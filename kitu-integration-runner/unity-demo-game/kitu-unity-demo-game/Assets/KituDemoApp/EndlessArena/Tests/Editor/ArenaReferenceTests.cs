using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using NUnit.Framework;
using UnityEngine;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaReferenceTests
    {
        internal static readonly string FixtureRoot = Path.GetFullPath(Path.Combine(Application.dataPath,
            "../../../../kitu-integration-runner/scenarios/arena/reference"));

        [Test]
        public void CaptureDetachesEntitiesItemsAndRecordedCommands()
        {
            var session = new ArenaReferenceSession("capture");
            var command = Command(1, "start");
            var first = session.Tick(default, command);
            command.address = "changed";
            session.Model.Inventory.ApplyDamage(10);
            first.inventory.equipment[0].name = "changed";
            Assert.That(session.Model.Inventory.Equipment[0].Name, Is.EqualTo("Blade"));
            Assert.That(first.inventory.health, Is.EqualTo(100));
            Assert.That(session.Recording.steps[0].commands[0].address, Is.EqualTo("/input/arena/start"));
            session.Model.TryAdvanceFloor();
            for (int i = 0; i < 20; i++) session.Tick(default);
            var captured = session.Model.CaptureReferenceState(0, 0, "none");
            int health = captured.enemies[0].Health;
            session.Model.Enemies[0].Health--;
            Assert.That(captured.enemies[0].Health, Is.EqualTo(health));
        }

        [Test]
        public void PauseConsumesCommandsWithoutAdvancingAnyGameplayClock()
        {
            var session = new ArenaReferenceSession();
            session.Tick(default, Command(1, "start"));
            var before = session.Tick(default, Command(2, "pause"));
            var after = session.Tick(new ArenaInput { Move = Vector2.up, FireA = true, UseA = true });
            Assert.That(after.elapsed, Is.EqualTo(before.elapsed));
            Assert.That(after.simulationSteps, Is.EqualTo(before.simulationSteps));
            Assert.That(after.playerPosition, Is.EqualTo(before.playerPosition));
            Assert.That(after.tick, Is.EqualTo(before.tick + 1));
            Assert.That(session.Tick(default, Command(3, "resume")).simulationSteps,
                Is.EqualTo(before.simulationSteps + 1));
        }

        [Test]
        public void DuplicatesKeepTheirOriginalResultAndStaleItemsCannotMove()
        {
            var session = new ArenaReferenceSession();
            session.Tick(default, Command(1, "start"), Command(2, "inventory"));
            session.Tick(default, Command(3, "discard", item: 99));
            Assert.That(session.Outcomes.Single().code, Is.EqualTo("stale_item"));
            session.Tick(default, Command(3, "discard", item: 99));
            Assert.That(session.Outcomes.Single().duplicate, Is.True);
            Assert.That(session.Outcomes.Single().accepted, Is.False);
            session.Tick(default, Command(1, "start"));
            Assert.That(session.Outcomes.Single().accepted, Is.True);
            Assert.That(session.Outcomes.Single().appliedTick, Is.Zero);
            Assert.That(session.Outcomes.Single().tick, Is.GreaterThan(0));
            Assert.That(session.Model.Floor, Is.Zero);
            Assert.That(session.Overlay, Is.EqualTo("inventory"));
            session.Tick(default, Command(1, "menu"));
            Assert.That(session.Outcomes.Single().code, Is.EqualTo("id_conflict"));
            Assert.That(session.Model.Phase, Is.EqualTo(ArenaPhase.Preparing));
        }

        [Test]
        public void SuccessfulUpgradeIsNotConsumedTwiceAfterRetry()
        {
            var session = new ArenaReferenceSession();
            session.Tick(default, Command(1, "start"));
            while (Vector2.Distance(session.Model.PlayerPosition, ArenaSimulation.ChestPosition) > 1.5f)
                session.Tick(new ArenaInput { Move = (ArenaSimulation.ChestPosition - session.Model.PlayerPosition).normalized });
            session.Tick(default, Command(2, "chest"));
            int item = session.Model.Inventory.Chest.Single(i => i.Kind == ItemKind.HealthUpgrade).Id;
            session.Tick(default, Command(3, "take", item));
            var upgrade = Command(4, "upgrade", item);
            session.Tick(default, upgrade);
            session.Tick(default, upgrade);
            Assert.That(session.Outcomes.Single().duplicate, Is.True);
            Assert.That(session.Outcomes.Single().accepted, Is.True);
            Assert.That(session.Model.Inventory.MaxHealth, Is.EqualTo(110));
            Assert.That(session.Model.Inventory.Backpack[0], Is.Null);
        }

        [Test]
        public void InvalidBatchesAndNonContiguousTicksFailBeforeStateMutation()
        {
            var session = new ArenaReferenceSession("invalid");
            Assert.Throws<ArgumentException>(() => session.Tick(default, Command(1, "start"), Command(0, "pause")));
            Assert.Throws<ArgumentException>(() => session.Tick(new ArenaInput { Move = new Vector2(float.NaN, 0) }));
            Assert.Throws<ArgumentException>(() => session.Replay(new ArenaReferenceStep { tick = 1 }));
            Assert.That(session.NextTick, Is.Zero);
            Assert.That(session.Recording.steps, Is.Empty);
            Assert.That(session.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
        }

        [TestCase("preparation")]
        [TestCase("stock-eleven-death-retry")]
        public void FrozenTraceReplaysThroughOriginalRules(string name)
        {
            var trace = JsonUtility.FromJson<ArenaReferenceTrace>(File.ReadAllText(Path.Combine(FixtureRoot, name, "scenario.json")));
            Assert.That(trace.schemaVersion, Is.EqualTo(1));
            Assert.That(trace.tickRate, Is.EqualTo(60));
            Assert.That(trace.baselineRevision, Is.EqualTo(ArenaReferenceSession.BaselineRevision));
            var states = File.ReadLines(Path.Combine(FixtureRoot, name, "expected.ndjson"))
                .Select(line => JsonUtility.FromJson<ArenaReferenceState>(line)).ToDictionary(state => state.tick);
            var results = File.ReadLines(Path.Combine(FixtureRoot, name, "outcomes.ndjson"))
                .Select(line => JsonUtility.FromJson<ArenaCommandOutcome>(line)).ToArray();
            var actualResults = new List<ArenaCommandOutcome>();
            var session = new ArenaReferenceSession();
            int compared = 0;
            foreach (var step in trace.steps)
            {
                var actual = session.Replay(step);
                actualResults.AddRange(session.Outcomes);
                if (!states.TryGetValue(actual.tick, out var expected)) continue;
                // This is the unchanged C# oracle on the same runtime, so require exact values.
                // The Rust differential runner applies the documented numeric tolerance by field.
                Assert.That(JsonUtility.ToJson(actual), Is.EqualTo(JsonUtility.ToJson(expected)),
                    name + ": snapshot differs at tick " + actual.tick);
                compared++;
            }
            Assert.That(compared, Is.EqualTo(states.Count));
            Assert.That(actualResults.Select(JsonUtility.ToJson), Is.EqualTo(results.Select(JsonUtility.ToJson)));
            if (name == "stock-eleven-death-retry")
            {
                Assert.That(states.Values.Any(s => s.result.present && s.result.floor == 11), Is.True);
                Assert.That(states.Values.Any(s => s.floor == 11 && s.inventory.maxHealth == 130 && s.bossesDefeated == 2), Is.True);
                Assert.That(session.Model.Phase, Is.EqualTo(ArenaPhase.Preparing));
                Assert.That(session.Model.Inventory.MaxHealth, Is.EqualTo(100));
                Assert.That(session.Model.Result, Is.Null);
            }
        }

        internal static ArenaReferenceCommand Command(long id, string name, int item = 0, int index = 0, int slot = 0)
            => new ArenaReferenceCommand { id = id, address = "/input/arena/" + name, itemId = item, index = index, slot = slot };
    }
}
