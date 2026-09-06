using System;
using Newtonsoft.Json.Linq;
using UnityEngine;
using UnityEngine.InputSystem;

namespace UnityOnlyArena
{
    // An input/presentation client. ArenaSimulation is never constructed here.
    public sealed class KituArenaClient : MonoBehaviour
    {
        public string Endpoint = "ws://127.0.0.1:8787/ws/runtime";
        public bool ConnectOnStart = true;
        public bool DeviceInput = true;
        private readonly string clientId = Guid.NewGuid().ToString("N");
        private long nextMessageId = 1;
        private ArenaConnection connection;
        private ArenaWorldView world;
        private float nextFrame;
        private readonly bool[] requireRelease = { true, true, true, true };
        private bool synchronized;
        private int selectedBackpack;
        private Vector2 inventoryScroll;
        public ArenaSettings Settings { get; private set; }
        public ArenaSettings DraftSettings { get; private set; }
        public bool SettingsOpen => DraftSettings != null;
        public bool ReplayActive { get; private set; }
        public bool ReplayPlaying { get; private set; }
        public string SessionId { get; private set; }
        public ArenaReferenceState State { get; private set; }
        public string Message { get; private set; } = "";
        public bool Connected => connection != null && connection.Connected && synchronized;
        public Camera GameCamera => world == null ? null : world.GameCamera;

        private void Awake()
        {
            Settings = ArenaSettings.Load();
            Settings.Apply(false);
            State = new ArenaReferenceState { tick = -1, aimDirection = Vector2.up, overlay = "none" };
            world = gameObject.AddComponent<ArenaWorldView>();
            world.Initialize(State);
        }

        private void Start() { if (ConnectOnStart) Connect(); }

        public void Connect()
        {
            connection?.Dispose();
            SessionId = null;
            synchronized = false;
            ReplayActive = false;
            BlockGameplayButtons();
            connection = new ArenaConnection(Endpoint);
        }

        public void Disconnect() { connection?.Dispose(); synchronized = false; }

        public bool Command(string suffix, params int[] values)
        {
            if (SettingsOpen) return false;
            var args = new JArray();
            foreach (int value in values) args.Add(Arg("int", value));
            return Send("/input/arena/" + suffix, args);
        }

        public bool Frame(Vector2 move, Vector2 aim, bool hasAim = true, bool fireA = false, bool fireB = false)
        {
            return Send("/input/arena/frame", new JArray(Arg("float", move.x), Arg("float", move.y),
                Arg("bool", hasAim), Arg("float", aim.x), Arg("float", aim.y), Arg("bool", fireA), Arg("bool", fireB)));
        }

        private static JObject Arg(string type, object value) => new JObject { ["type"] = type, ["value"] = JToken.FromObject(value) };
        private bool Send(string address, JArray args)
        {
            if (!Connected || ReplayActive) return false;
            var envelope = new JObject { ["schemaVersion"] = 1, ["sessionId"] = SessionId,
                ["clientId"] = clientId, ["messageId"] = nextMessageId++, ["address"] = address, ["args"] = args };
            return connection.Send(envelope.ToString(Newtonsoft.Json.Formatting.None));
        }

        private void Update()
        {
            if (connection == null) return;
            while (connection.TryReceive(out string json))
            {
                try
                {
                    var message = JObject.Parse(json);
                    switch ((string)message["type"])
                    {
                        case "replay":
                            bool active = (bool)message["mode"]["active"];
                            if (active != ReplayActive) { BlockGameplayButtons(); DraftSettings = null; }
                            ReplayActive = active;
                            ReplayPlaying = (bool)message["mode"]["playing"];
                            break;
                        case "arenaSession":
                            if ((int)message["schemaVersion"] != 1) throw new InvalidOperationException("Incompatible Arena contract");
                            SessionId = (string)message["id"];
                            synchronized = false;
                            break;
                        case "osc":
                            if ((string)message["address"] == "/ui/arena/state" && SessionId != null)
                            {
                                var state = JsonUtility.FromJson<ArenaReferenceState>((string)message["args"][0]["value"]);
                                if (state.overlay != State.overlay ||
                                    (state.phase != State.phase && (state.phase == 0 || state.phase == 5 || State.phase == 0 || State.phase == 5)))
                                    BlockGameplayButtons();
                                if (SettingsOpen && state.phase != 0 && state.overlay != "pause") DraftSettings = null;
                                State = state;
                                synchronized = true;
                                world.Sync(State);
                            }
                            else if ((string)message["address"] == "/ui/arena/use")
                            {
                                var result = JObject.Parse((string)message["args"][0]["value"]);
                                Message = (string)result["code"];
                            }
                            else if ((string)message["address"] == "/ui/arena/command")
                            {
                                var outcome = JObject.Parse((string)message["args"][0]["value"]);
                                Message = (string)outcome["code"];
                            }
                            break;
                        case "error": Message = (string)message["message"]; break;
                    }
                }
                catch (Exception error) { Message = error.Message; Disconnect(); break; }
            }
            if (!Connected) { BlockGameplayButtons(); return; }
            if (DeviceInput && !ReplayActive) ReadInput();
        }

        private void ReadInput()
        {
            var keyboard = Keyboard.current;
            if (SettingsOpen)
            {
                if (keyboard != null && keyboard.escapeKey.wasPressedThisFrame) CancelSettings();
                return;
            }
            if (keyboard != null)
            {
                if (keyboard.escapeKey.wasPressedThisFrame)
                    Command(State.overlay == "inventory" || State.overlay == "chest" ? "close" : State.overlay == "pause" ? "resume" : "pause");
                if (keyboard.tabKey.wasPressedThisFrame || keyboard.iKey.wasPressedThisFrame) Command(State.overlay == "inventory" || State.overlay == "chest" ? "close" : "inventory");
                if (keyboard.eKey.wasPressedThisFrame && State.overlay == "none") Command("chest");
                if (keyboard.rKey.wasPressedThisFrame && State.phase == 5) Command("start");
            }
            var mouse = Mouse.current;
            bool[] down = { mouse != null && mouse.leftButton.isPressed, mouse != null && mouse.rightButton.isPressed,
                keyboard != null && keyboard.zKey.isPressed, keyboard != null && keyboard.xKey.isPressed };
            for (int i = 0; i < down.Length; i++) if (!down[i]) requireRelease[i] = false;
            bool running = State.overlay == "none" && State.phase != 0 && State.phase != 5;
            bool useA = running && keyboard != null && !requireRelease[2] && keyboard.zKey.wasPressedThisFrame;
            bool useB = running && keyboard != null && !requireRelease[3] && keyboard.xKey.wasPressedThisFrame;
            // A press forces a fresh aim frame before the discrete use request.
            // This timer samples devices; only the server owns the game clock.
            if (Time.unscaledTime < nextFrame && !useA && !useB) return;
            nextFrame = Time.unscaledTime + 1f / 60f;
            var move = Vector2.zero;
            if (keyboard != null && State.overlay == "none")
            {
                float x = (keyboard.dKey.isPressed ? 1 : 0) - (keyboard.aKey.isPressed ? 1 : 0);
                float y = (keyboard.wKey.isPressed ? 1 : 0) - (keyboard.sKey.isPressed ? 1 : 0);
                Vector3 right = GameCamera.transform.right, forward = GameCamera.transform.forward;
                right.y = forward.y = 0;
                Vector3 direction = right.normalized * x + forward.normalized * y;
                move = new Vector2(direction.x, direction.z);
            }
            Vector2 aim = Vector2.zero;
            bool hasAim = false;
            if (mouse != null)
            {
                var ray = GameCamera.ScreenPointToRay(mouse.position.ReadValue());
                if (new Plane(Vector3.up, Vector3.zero).Raycast(ray, out float distance))
                { var point = ray.GetPoint(distance); aim = new Vector2(point.x, point.z); hasAim = true; }
            }
            Frame(move, aim, hasAim, running && !requireRelease[0] && down[0], running && !requireRelease[1] && down[1]);
            if (useA) Command("use", 2);
            if (useB) Command("use", 3);
        }

        private void BlockGameplayButtons()
        {
            for (int i = 0; i < requireRelease.Length; i++) requireRelease[i] = true;
        }

        private void OnApplicationFocus(bool focused) { if (!focused) { Command("pause"); BlockGameplayButtons(); } }
        private void OnDestroy() { connection?.Dispose(); }

        private static string ItemText(ArenaItemState item)
        {
            if (item == null || item.id == 0) return "Empty";
            string detail = item.kind == (int)ItemKind.Shield ? $" · Shield {item.shield}" :
                item.kind <= (int)ItemKind.HeavyShooter ? $" · {item.damage} damage / {item.interval:0.##}s" : "";
            return item.name + detail;
        }

        private void DrawInventory()
        {
            var inventory = State.inventory;
            var area = new Rect(Screen.width * .12f, 170, Screen.width * .76f, Mathf.Max(100, Screen.height - 185));
            GUI.Box(area, "");
            GUILayout.BeginArea(new Rect(area.x + 15, area.y + 10, area.width - 30, area.height - 20));
            GUILayout.BeginHorizontal();
            GUILayout.Label(State.overlay == "chest" ? "SUPPLY CHEST · game paused" : "INVENTORY · game paused");
            if (GUILayout.Button("Close", GUILayout.Width(100))) Command("close");
            GUILayout.EndHorizontal();
            inventoryScroll = GUILayout.BeginScrollView(inventoryScroll);
            GUILayout.Label("Backpack · choose a destination for taking or swapping items");
            for (int i = 0; i < inventory.backpack.Length; i++)
            {
                GUILayout.BeginHorizontal();
                if (GUILayout.Toggle(selectedBackpack == i, $"{i + 1}. {ItemText(inventory.backpack[i])}", "Button")) selectedBackpack = i;
                var item = inventory.backpack[i];
                if (item.id != 0)
                {
                    if (item.kind == (int)ItemKind.HealthUpgrade || item.kind == (int)ItemKind.AttackUpgrade)
                    { if (GUILayout.Button("Use upgrade", GUILayout.Width(110))) Command("upgrade", item.id, i); }
                    if (GUILayout.Button("Discard", GUILayout.Width(80))) Command("discard", item.id, i);
                }
                GUILayout.EndHorizontal();
            }
            GUILayout.Space(10);
            GUILayout.Label("Equipment · swaps preserve the outgoing item in the selected backpack slot");
            for (int slot = 0; slot < inventory.equipment.Length; slot++)
            {
                GUILayout.BeginHorizontal();
                var item = inventory.equipment[slot];
                string name = slot < 2 ? "Weapon " + (slot == 0 ? "A" : "B") : "Item " + (slot == 2 ? "A" : "B");
                GUILayout.Label(name + ": " + ItemText(item));
                var selected = inventory.backpack[selectedBackpack];
                if (selected.id != 0 && GUILayout.Button("Equip selected", GUILayout.Width(120))) Command("equip", selected.id, selectedBackpack, slot);
                if (item.id != 0 && GUILayout.Button("Unequip", GUILayout.Width(85))) Command("unequip", item.id, slot);
                GUILayout.EndHorizontal();
            }
            if (State.overlay == "chest")
            {
                GUILayout.Space(10);
                GUILayout.Label("Chest");
                foreach (var item in inventory.chest)
                {
                    GUILayout.BeginHorizontal(); GUILayout.Label(ItemText(item));
                    if (GUILayout.Button(inventory.backpack[selectedBackpack].id == 0 ? "Take" : "Swap", GUILayout.Width(100)))
                        Command("take", item.id, selectedBackpack);
                    GUILayout.EndHorizontal();
                }
            }
            GUILayout.EndScrollView();
            GUILayout.EndArea();
        }

        public void OpenSettings()
        {
            if (ReplayActive) return;
            if (State.phase != 0 && State.overlay != "pause") return;
            DraftSettings = Settings.Copy();
            BlockGameplayButtons();
        }

        public void ApplySettings()
        {
            if (!SettingsOpen) return;
            Settings = DraftSettings.Copy();
            Settings.Apply();
            DraftSettings = null;
            BlockGameplayButtons();
        }

        public void CancelSettings() { DraftSettings = null; BlockGameplayButtons(); }
        public void ResetSettingsDraft() { if (SettingsOpen) DraftSettings = new ArenaSettings(); }

        private void DrawSettings()
        {
            var area = new Rect((Screen.width - 550) / 2f, 175, 550, 335);
            GUI.Box(area, "");
            GUILayout.BeginArea(new Rect(area.x + 20, area.y + 15, area.width - 40, area.height - 30));
            GUILayout.Label("SETTINGS");
            GUILayout.Space(15);
            GUILayout.Label($"Master volume: {Mathf.RoundToInt(DraftSettings.Volume * 100)}%");
            DraftSettings.Volume = GUILayout.HorizontalSlider(DraftSettings.Volume, 0, 1);
            DraftSettings.Fullscreen = GUILayout.Toggle(DraftSettings.Fullscreen, "Borderless fullscreen (standalone)");
            GUILayout.Label("Changes are saved only when applied.");
            GUILayout.Space(15);
            if (GUILayout.Button("Restore defaults")) ResetSettingsDraft();
            if (GUILayout.Button("Apply and return")) ApplySettings();
            if (GUILayout.Button("Cancel")) CancelSettings();
            GUILayout.EndArea();
        }

        private void DrawResults()
        {
            var result = State.result;
            if (result == null || !result.present) return;
            var area = new Rect((Screen.width - 660) / 2f, 175, 660, 370);
            GUI.Box(area, "");
            GUILayout.BeginArea(new Rect(area.x + 20, area.y + 15, area.width - 40, area.height - 30));
            GUILayout.Label("GAME OVER");
            GUILayout.Label($"Reached {result.floor}F · Cleared {result.floorsCleared} floors");
            GUILayout.Label($"Defeated {result.enemiesDefeated} enemies, including {result.bossesDefeated} bosses");
            GUILayout.Label($"Run time {result.elapsed:F2}s · Maximum HP {result.maxHealth} · Attack ×{result.attackMultiplier:F2}");
            string[] names = { "Weapon A", "Weapon B", "Item A", "Item B" };
            for (int i = 0; i < result.equipmentNames.Length; i++) GUILayout.Label(names[i] + ": " + result.equipmentNames[i]);
            GUILayout.Space(15);
            if (GUILayout.Button("Try again (R)")) Command("start");
            if (GUILayout.Button("Return to menu")) Command("menu");
            GUILayout.EndArea();
        }

        private void OnGUI()
        {
            GUILayout.BeginArea(new Rect(20, 12, Screen.width - 40, 150));
            GUILayout.Label($"ENDLESS ARENA · {State.floor}F · {(ArenaPhase)State.phase} · Enemies {State.enemies?.Length ?? 0}");
            GUILayout.Label($"{(Connected ? "Connected" : connection?.Status ?? "Disconnected")}  |  Tick {State.tick}  |  Time {State.elapsed:F2}  |  {State.overlay}  {Message}");
            GUILayout.BeginHorizontal();
            if (!Connected) { if (GUILayout.Button("Connect", GUILayout.Width(140))) Connect(); }
            else if (!SettingsOpen && !ReplayActive)
            {
                if (State.phase == 0 || State.phase == 5) { if (GUILayout.Button("Start run", GUILayout.Width(140))) Command("start"); }
                else if (GUILayout.Button(State.overlay == "pause" ? "Resume" : "Pause", GUILayout.Width(140))) Command(State.overlay == "pause" ? "resume" : "pause");
                if (State.overlay == "none" && (State.phase == 1 || State.phase == 4))
                {
                    if (GUILayout.Button("Inventory (I)", GUILayout.Width(140))) Command("inventory");
                    if (State.chestAvailable && GUILayout.Button("Open chest (E)", GUILayout.Width(140))) Command("chest");
                }
                if ((State.phase == 0 || State.overlay == "pause") && GUILayout.Button("Settings", GUILayout.Width(100))) OpenSettings();
                if (GUILayout.Button("Main menu", GUILayout.Width(140))) Command("menu");
                if (State.phase == 0 && GUILayout.Button("Quit", GUILayout.Width(80))) Application.Quit();
            }
            GUILayout.EndHorizontal();
            GUILayout.Label(ReplayActive ? "REPLAY · Read-only · Control playback and return to the live run from Admin" :
                "WASD: move · Mouse: aim/fire A/B · Z/X: items · Tab/I: inventory · E: chest · Escape: close/pause");
            if (State.inventory != null) GUILayout.Label($"HP {State.inventory.health}/{State.inventory.maxHealth} · Attack ×{State.inventory.attackMultiplier:F2}");
            string objective = State.phase == 1 ? "Prepare at the chest, then enter the portal" :
                State.phase == 3 ? (State.floor % 5 == 0 ? "Defeat the boss" : "Defeat every enemy") :
                State.phase == 4 ? (State.chestAvailable ? "Boss defeated · HP restored · Choose a reward, then enter the portal" : "Floor cleared · Enter the portal to continue") : "";
            GUILayout.Label(objective);
            GUILayout.EndArea();
            if (SettingsOpen) { DrawSettings(); return; }
            bool previousEnabled = GUI.enabled;
            if (ReplayActive) GUI.enabled = false;
            if (State.phase == 5) DrawResults();
            if (State.overlay == "none" && State.phase != 0 && State.phase != 5 && State.inventory != null)
            {
                GUILayout.BeginArea(new Rect(20, Screen.height - 115, Screen.width - 40, 105));
                for (int i = 0; i < State.inventory.equipment.Length; i++)
                    GUILayout.Label((i < 2 ? "Weapon " + (i == 0 ? "A" : "B") : "Item " + (i == 2 ? "A" : "B")) + ": " + ItemText(State.inventory.equipment[i]));
                GUILayout.EndArea();
            }
            if ((State.overlay == "inventory" || State.overlay == "chest") && State.inventory != null) DrawInventory();
            GUI.enabled = previousEnabled;
        }
    }
}
