using System;
using System.Collections.Generic;
using UnityEngine;

namespace UnityOnlyArena
{
    [Serializable]
    public sealed class ArenaReferenceCommand
    {
        public long id;
        public string address;
        public int itemId, index, slot;
    }

    [Serializable]
    public sealed class ArenaReferenceStep
    {
        public long tick;
        public ArenaInput frame;
        public ArenaReferenceCommand[] commands = Array.Empty<ArenaReferenceCommand>();
    }

    [Serializable]
    public sealed class ArenaCommandOutcome
    {
        public long id, tick, appliedTick;
        public bool accepted, duplicate;
        public string code;
    }

    [Serializable]
    public sealed class ArenaReferenceTrace
    {
        public int schemaVersion = 1, tickRate = 60;
        public string scenarioId;
        public string baselineRevision = ArenaReferenceSession.BaselineRevision;
        public List<ArenaReferenceStep> steps = new List<ArenaReferenceStep>();
    }

    /// <summary>
    /// An opt-in command driver around the unchanged Unity-only rules. Records effective
    /// tick inputs (after device sampling), never direct state edits. The existing ArenaGame
    /// remains the production input/UI oracle; this adapter makes headless rule traces portable.
    /// </summary>
    public sealed class ArenaReferenceSession
    {
        public const string BaselineRevision = "38f2b4be4b7b2b604f1b21dfe9ce407846e0fc43";
        public const float StepSeconds = 1f / 60f;
        public ArenaSimulation Model { get; } = new ArenaSimulation();
        public string Overlay { get; private set; } = "none";
        public long NextTick { get; private set; }
        public long SimulationSteps { get; private set; }
        public ArenaReferenceTrace Recording { get; }
        public IReadOnlyList<ArenaCommandOutcome> Outcomes => outcomes;
        private readonly List<ArenaCommandOutcome> outcomes = new List<ArenaCommandOutcome>();
        private readonly Dictionary<long, string> seen = new Dictionary<long, string>();
        private readonly Dictionary<long, ArenaCommandOutcome> previous = new Dictionary<long, ArenaCommandOutcome>();

        public ArenaReferenceSession(string recordingId = null)
        {
            if (recordingId != null) Recording = new ArenaReferenceTrace { scenarioId = recordingId };
        }

        /// <summary>Applies a committed control batch, then advances gameplay only when unpaused.</summary>
        public ArenaReferenceState Tick(ArenaInput frame, params ArenaReferenceCommand[] commands)
        {
            if (!Finite(frame.Move) || !Finite(frame.AimPoint))
                throw new ArgumentException("Reference frames must contain finite coordinates.");
            if (commands == null) throw new ArgumentNullException(nameof(commands));
            foreach (var command in commands)
                if (command == null || command.id <= 0 || command.address == null)
                    throw new ArgumentException("Commands require a positive id and address.");

            // Detach before execution: later edits by a caller cannot rewrite recorded history.
            var step = new ArenaReferenceStep { tick = NextTick, frame = frame, commands = commands };
            step = JsonUtility.FromJson<ArenaReferenceStep>(JsonUtility.ToJson(step));
            Recording?.steps.Add(step);
            outcomes.Clear();
            foreach (var command in step.commands) Apply(command);
            if (Overlay == "none" && Model.Phase != ArenaPhase.Opening && Model.Phase != ArenaPhase.Results)
            {
                Model.Step(StepSeconds, step.frame);
                SimulationSteps++;
            }
            var state = Model.CaptureReferenceState(NextTick, SimulationSteps, Overlay);
            NextTick++;
            return state;
        }

        /// <summary>Replays exactly one contiguous recorded tick through the ordinary command driver.</summary>
        public ArenaReferenceState Replay(ArenaReferenceStep step)
        {
            if (step == null || step.tick != NextTick)
                throw new ArgumentException("Trace ticks must be contiguous and start at zero.");
            return Tick(step.frame, step.commands);
        }

        private void Apply(ArenaReferenceCommand command)
        {
            string fingerprint = JsonUtility.ToJson(command);
            if (seen.TryGetValue(command.id, out string original))
            {
                var result = previous[command.id];
                outcomes.Add(new ArenaCommandOutcome { id = command.id, tick = NextTick,
                    appliedTick = original == fingerprint ? result.appliedTick : -1,
                    accepted = original == fingerprint && result.accepted, duplicate = true,
                    code = original == fingerprint ? result.code : "id_conflict" });
                return;
            }
            seen.Add(command.id, fingerprint);
            string code = Execute(command);
            var outcome = new ArenaCommandOutcome { id = command.id, tick = NextTick,
                appliedTick = NextTick,
                accepted = code == "ok", code = code };
            previous.Add(command.id, outcome);
            outcomes.Add(outcome);
        }

        private string Execute(ArenaReferenceCommand c)
        {
            switch (c.address)
            {
                case "/input/arena/start":
                    if (Model.Phase != ArenaPhase.Opening && Model.Phase != ArenaPhase.Results) return "invalid_state";
                    Model.StartRun(); Overlay = "none"; return "ok";
                case "/input/arena/menu":
                    Model.ReturnToMenu(); Overlay = "none"; return "ok";
                case "/input/arena/pause":
                case "/input/arena/disconnect":
                    if (!HasRun) return "invalid_state";
                    Overlay = "pause"; return "ok";
                case "/input/arena/resume":
                    if (!HasRun || Overlay != "pause") return "invalid_state";
                    Overlay = "none"; return "ok";
                case "/input/arena/inventory":
                    if (!Model.IsSafe || Overlay != "none") return "invalid_state";
                    Overlay = "inventory"; return "ok";
                case "/input/arena/chest":
                    if (!Model.IsSafe || Overlay != "none") return "invalid_state";
                    if (!Model.ChestAvailable || Vector2.Distance(Model.PlayerPosition, ArenaSimulation.ChestPosition) > 2f)
                        return "out_of_range";
                    Overlay = "chest"; return "ok";
                case "/input/arena/close":
                    if (Overlay != "inventory" && Overlay != "chest") return "invalid_state";
                    Overlay = "none"; return "ok";
                case "/input/arena/take":
                case "/input/arena/equip":
                case "/input/arena/unequip":
                case "/input/arena/discard":
                case "/input/arena/upgrade": return InventoryCommand(c);
                default: return "unknown_command";
            }
        }

        private bool HasRun => Model.Phase != ArenaPhase.Opening && Model.Phase != ArenaPhase.Results;

        private string InventoryCommand(ArenaReferenceCommand c)
        {
            if (!Model.IsSafe || (Overlay != "inventory" && Overlay != "chest")) return "invalid_state";
            var inventory = Model.Inventory;
            bool accepted;
            if (c.address == "/input/arena/take")
            {
                if (Overlay != "chest") return "invalid_state";
                int chestIndex = inventory.Chest.FindIndex(item => item.Id == c.itemId);
                if (c.itemId <= 0 || chestIndex < 0) return "stale_item";
                accepted = inventory.TakeChest(chestIndex, c.index);
            }
            else if (c.address == "/input/arena/unequip")
            {
                if (c.slot < 0 || c.slot >= inventory.Equipment.Length) return "invalid_target";
                if (c.itemId <= 0 || inventory.Equipment[c.slot]?.Id != c.itemId) return "stale_item";
                accepted = inventory.Unequip((EquipmentSlot)c.slot);
            }
            else
            {
                if (c.index < 0 || c.index >= inventory.Backpack.Length) return "invalid_target";
                if (c.itemId <= 0 || inventory.Backpack[c.index]?.Id != c.itemId) return "stale_item";
                switch (c.address)
                {
                    case "/input/arena/equip": accepted = inventory.Equip(c.index, (EquipmentSlot)c.slot); break;
                    case "/input/arena/discard": accepted = inventory.Discard(c.index); break;
                    default: accepted = inventory.UseUpgrade(c.index); break;
                }
            }
            return accepted ? "ok" : "rule_rejected";
        }

        private static bool Finite(Vector2 v) => !float.IsNaN(v.x) && !float.IsInfinity(v.x)
            && !float.IsNaN(v.y) && !float.IsInfinity(v.y);
    }
}
