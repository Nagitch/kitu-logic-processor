using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using UnityEngine;

namespace UnityOnlyArena.Tests
{
    /// <summary>Explicit fixture export only; normal tests never rewrite expected results.</summary>
    public static class ArenaReferenceFixtureBuilder
    {
        public static void Export()
        {
            string root = Environment.GetEnvironmentVariable("KITU_ARENA_REFERENCE_OUTPUT");
            if (string.IsNullOrEmpty(root)) root = Path.GetFullPath(Path.Combine(Application.dataPath, "../Logs/arena-reference"));
            var preparation = new Capture("preparation");
            preparation.Step(default, preparation.Command("start"));
            preparation.Step(default, preparation.Command("inventory"));
            preparation.Step(default, preparation.Command("unequip", item: 1, slot: 0));
            preparation.Step(default, preparation.Command("equip", item: 1, slot: 0));
            preparation.Step(default, preparation.Command("discard", item: 999));
            var close = preparation.Command("close");
            preparation.Step(default, close);
            preparation.Step(default, close);
            preparation.Step(default, preparation.Command("pause"));
            for (int i = 0; i < 4; i++) preparation.Step(new ArenaInput { Move = Vector2.up, FireA = true });
            preparation.Step(default, preparation.Command("resume"));
            for (int i = 0; i < 12; i++) preparation.Step(new ArenaInput { Move = Vector2.right, HasAim = true, AimPoint = Vector2.right * 8 });
            preparation.Step(default, preparation.Command("disconnect"));
            preparation.Step(default, preparation.Command("resume"));
            preparation.Step(default, preparation.Command("menu"));
            preparation.Write(root);

            var capture = new Capture("stock-eleven-death-retry");
            capture.Step(default, capture.Command("start"));
            int preparedFloor = -1, observedClears = 0;
            bool exitPortal = false;
            for (int tick = 0; tick < 60 * 60 * 20; tick++)
            {
                var game = capture.Session.Model;
                if (game.Phase == ArenaPhase.Results)
                {
                    if (game.Floor != 11) throw new InvalidOperationException("Stock reference died before reaching 11F.");
                    capture.Step(default, capture.Command("start"));
                    capture.Write(root);
                    Debug.Log("Arena reference fixtures exported to " + root);
                    return;
                }
                if (game.FloorsCleared != observedClears)
                {
                    observedClears = game.FloorsCleared;
                    exitPortal = Vector2.Distance(game.PlayerPosition, ArenaSimulation.PortalPosition) <= ArenaSimulation.PortalRadius;
                }
                var input = new ArenaInput();
                if (game.Floor >= 11 && game.Phase == ArenaPhase.Combat)
                {
                    if (game.Inventory.MaxHealth != 130 || game.BossesDefeated != 2)
                        throw new InvalidOperationException("Stock progression did not preserve rewards.");
                    // Natural death: no debug damage, world patches or edited statistics.
                }
                else if (game.IsSafe)
                {
                    if (exitPortal)
                    {
                        input.Move = Vector2.down;
                        if (Vector2.Distance(game.PlayerPosition, ArenaSimulation.PortalPosition) > ArenaSimulation.PortalRadius + .25f)
                            exitPortal = false;
                    }
                    else if (game.ChestAvailable && preparedFloor != game.Floor)
                    {
                        Vector2 toChest = ArenaSimulation.ChestPosition - game.PlayerPosition;
                        if (toChest.magnitude > 1.5f) input.Move = toChest.normalized;
                        else { capture.StockLoadout(); preparedFloor = game.Floor; }
                    }
                    else input.Move = (ArenaSimulation.PortalPosition - game.PlayerPosition).normalized;
                }
                else if (game.Phase == ArenaPhase.Combat)
                {
                    float radius = game.PlayerPosition.magnitude;
                    Vector2 radial = radius > .01f ? game.PlayerPosition / radius : Vector2.down;
                    input.Move = (new Vector2(-radial.y, radial.x) + radial * ((7.7f - radius) * 1.5f)).normalized;
                    input.HasAim = true;
                    input.AimPoint = game.Enemies.OrderBy(e => (e.Position - game.PlayerPosition).sqrMagnitude).First().Position;
                    input.FireA = input.FireB = true;
                    input.UseB = game.Inventory.Health <= game.Inventory.MaxHealth / 2;
                }
                capture.Step(input);
            }
            throw new InvalidOperationException("Stock reference exceeded its tick budget.");
        }

        private sealed class Capture
        {
            public readonly ArenaReferenceSession Session;
            private long nextId = 1;
            private string discreteState;
            private readonly List<ArenaReferenceState> states = new List<ArenaReferenceState>();
            private readonly List<ArenaCommandOutcome> outcomes = new List<ArenaCommandOutcome>();
            private ArenaReferenceState last;
            public Capture(string name) { Session = new ArenaReferenceSession(name); }
            public ArenaReferenceCommand Command(string name, int item = 0, int index = 0, int slot = 0)
                => ArenaReferenceTests.Command(nextId++, name, item, index, slot);

            public void Step(ArenaInput frame, params ArenaReferenceCommand[] commands)
            {
                last = Session.Tick(frame, commands);
                outcomes.AddRange(Session.Outcomes);
                if (Session.Recording.scenarioId == "stock-eleven-death-retry" && Session.Outcomes.Any(o => !o.accepted))
                    throw new InvalidOperationException("Stock command rejected at tick " + last.tick);
                string key = last.phase + ":" + last.floor + ":" + last.inventory.health + ":" + last.floorsCleared
                    + ":" + string.Join(",", last.enemies.Select(e => e.Id + "/" + e.Health + "/" + (int)e.BossState))
                    + ":" + string.Join(",", last.inventory.equipment.Select(i => i.id + "/" + i.shield));
                if (commands.Length > 0 || last.tick % 60 == 0 || key != discreteState) states.Add(last);
                discreteState = key;
            }

            public void StockLoadout()
            {
                Step(default, Command("chest"));
                Equip("Power Shooter", EquipmentSlot.WeaponA);
                Equip("Quick Shooter", EquipmentSlot.WeaponB);
                Equip("Shield", EquipmentSlot.ItemA);
                Equip("Medkit", EquipmentSlot.ItemB);
                foreach (var kind in new[] { ItemKind.HealthUpgrade, ItemKind.AttackUpgrade })
                {
                    var inventory = Session.Model.Inventory;
                    int item = inventory.Chest.First(i => i.Kind == kind).Id;
                    int index = Array.FindIndex(inventory.Backpack, i => i == null);
                    Step(default, Command("take", item, index));
                    Step(default, Command("upgrade", item, index));
                }
                Step(default, Command("close"));
                if (Session.Outcomes.Any(o => !o.accepted)) throw new InvalidOperationException("Failed to close stock chest.");
            }

            private void Equip(string name, EquipmentSlot slot)
            {
                var inventory = Session.Model.Inventory;
                int item = inventory.Chest.First(i => i.Name == name).Id;
                int index = Array.FindIndex(inventory.Backpack, i => i == null);
                Step(default, Command("take", item, index));
                Step(default, Command("equip", item, index, (int)slot));
                if (inventory.Backpack[index] != null) Step(default, Command("discard", inventory.Backpack[index].Id, index));
            }

            public void Write(string root)
            {
                if (states.Count == 0 || states.Last().tick != last.tick) states.Add(last);
                string directory = Path.Combine(root, Session.Recording.scenarioId);
                Directory.CreateDirectory(directory);
                // One effective tick per line keeps the generated input stream reviewable.
                File.WriteAllText(Path.Combine(directory, "scenario.json"),
                    JsonUtility.ToJson(Session.Recording).Replace("},{\"tick\":", "},\n{\"tick\":") + "\n");
                File.WriteAllLines(Path.Combine(directory, "expected.ndjson"), states.Select(s => JsonUtility.ToJson(s)));
                File.WriteAllLines(Path.Combine(directory, "outcomes.ndjson"), outcomes.Select(o => JsonUtility.ToJson(o)));
            }
        }
    }
}
