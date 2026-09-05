using System;
using System.Collections;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;
using System.Linq;
using UnityEngine.TestTools;
using UnityEngine.TestTools.Utils;

namespace UnityOnlyArena.Tests
{
    public sealed class KituArenaConnectionTests
    {
        private GameObject root;
        private Keyboard keyboard, previousKeyboard;
        private Mouse mouse, previousMouse;
        private bool inputConfigured, previousBackground;
        private InputSettings.UpdateMode previousUpdateMode;
        private InputSettings.BackgroundBehavior previousBackgroundBehavior;
        private InputSettings.EditorInputBehaviorInPlayMode previousEditorInput;
        [UnityTearDown]
        public IEnumerator Cleanup()
        {
            if (root != null) { root.SetActive(false); UnityEngine.Object.Destroy(root); }
            if (inputConfigured)
            {
                if (keyboard != null && keyboard.added) InputSystem.RemoveDevice(keyboard);
                if (mouse != null && mouse.added) InputSystem.RemoveDevice(mouse);
                InputSystem.settings.updateMode = previousUpdateMode;
                InputSystem.settings.backgroundBehavior = previousBackgroundBehavior;
                InputSystem.settings.editorInputBehaviorInPlayMode = previousEditorInput;
                Application.runInBackground = previousBackground;
                if (previousKeyboard != null && previousKeyboard.added) previousKeyboard.MakeCurrent();
                if (previousMouse != null && previousMouse.added) previousMouse.MakeCurrent();
                inputConfigured = false;
            }
            yield return null;
        }

        [UnityTest, Category("ArenaNetwork")]
        public IEnumerator RealRuntimeMovesPausesAndResynchronizesWithoutRunningLocalRules()
        {
            string endpoint = Environment.GetEnvironmentVariable("KITU_ARENA_WS_URL");
            if (string.IsNullOrEmpty(endpoint)) Assert.Ignore("Set KITU_ARENA_WS_URL to an isolated running admin host.");
            // Load the checked-in migration scene, then configure its client before Start.
            var path = "Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity";
#if UNITY_EDITOR
            yield return UnityEditor.SceneManagement.EditorSceneManager.LoadSceneAsyncInPlayMode(path,
                new LoadSceneParameters(LoadSceneMode.Single));
#else
            yield return SceneManager.LoadSceneAsync(path, LoadSceneMode.Single);
#endif
            var client = UnityEngine.Object.FindFirstObjectByType<KituArenaClient>();
            Assert.That(client, Is.Not.Null);
            root = client.gameObject;
            client.DeviceInput = false;
            client.Endpoint = endpoint;
            client.Connect();
            yield return Until(() => client.Connected, "initial synchronization");
            Assert.That(UnityEngine.Object.FindFirstObjectByType<ArenaGame>(), Is.Null);
            Assert.That(client.Command("menu"), Is.True);
            yield return Until(() => client.State.phase == 0, "opening");
            Assert.That(client.Command("start"), Is.True);
            yield return Until(() => client.State.phase == 1, "preparation");
            float start = client.State.playerPosition.x;
            client.Frame(Vector2.right, Vector2.zero);
            yield return Until(() => client.State.playerPosition.x > start + .3f, "server movement");
            client.Command("pause");
            yield return Until(() => client.State.overlay == "pause", "pause");
            var paused = client.State.playerPosition;
            float elapsed = client.State.elapsed;
            long tick = client.State.tick;
            yield return Until(() => client.State.tick > tick + 8, "management ticks while paused");
            Assert.That(client.State.elapsed, Is.EqualTo(elapsed));
            Assert.That(client.State.playerPosition, Is.EqualTo(paused).Using(Vector2ComparerWithEqualsOperator.Instance));
            string session = client.SessionId;
            client.Disconnect();
            yield return new WaitForSecondsRealtime(.15f);
            client.Connect();
            yield return Until(() => client.Connected, "reconnection");
            Assert.That(client.SessionId, Is.EqualTo(session));
            Assert.That(client.State.overlay, Is.EqualTo("pause"));
            Assert.That(client.State.elapsed, Is.EqualTo(elapsed));
            client.Command("resume");
            yield return Until(() => client.State.elapsed > elapsed + .1f, "explicit resume");
            Assert.That(client.State.playerPosition, Is.EqualTo(paused).Using(Vector2ComparerWithEqualsOperator.Instance));
            var player = root.transform.Find("Arena presentation/Player");
            Assert.That(player, Is.Not.Null);
            Assert.That(player.position.x, Is.EqualTo(client.State.playerPosition.x).Within(1e-4));
            // Interact through the same public commands used by the inventory UI.
            Assert.That(client.State.inventory.health, Is.EqualTo(100));
            float deadline = Time.realtimeSinceStartup + 8f;
            while (Vector2.Distance(client.State.playerPosition, ArenaSimulation.ChestPosition) > 1.3f && Time.realtimeSinceStartup < deadline)
            {
                client.Frame((ArenaSimulation.ChestPosition - client.State.playerPosition).normalized, Vector2.zero);
                yield return null;
            }
            client.Frame(Vector2.zero, Vector2.zero);
            client.Command("chest");
            yield return Until(() => client.State.overlay == "chest", "open real chest");
            float chestTime = client.State.elapsed;
            int shieldId = Array.Find(client.State.inventory.chest, item => item.kind == (int)ItemKind.Shield).id;
            client.Command("take", shieldId, 0);
            yield return Until(() => client.State.inventory.backpack[0].id == shieldId, "take shield");
            client.Command("equip", shieldId, 0, 2);
            yield return Until(() => client.State.inventory.equipment[2].id == shieldId, "equip shield");
            int upgradeId = Array.Find(client.State.inventory.chest, item => item.kind == (int)ItemKind.HealthUpgrade).id;
            client.Command("take", upgradeId, 0);
            yield return Until(() => client.State.inventory.backpack[0].id == upgradeId, "take upgrade");
            client.Command("upgrade", upgradeId, 0);
            yield return Until(() => client.State.inventory.maxHealth == 110, "consume upgrade");
            Assert.That(client.State.inventory.equipment[2].shield, Is.EqualTo(100));
            string unchanged = JsonUtility.ToJson(client.State.inventory);
            int incoming = client.State.inventory.chest[0].id;
            client.Command("take", incoming, 3);
            yield return Until(() => client.Message == "invalid_target", "reject invalid destination");
            Assert.That(JsonUtility.ToJson(client.State.inventory), Is.EqualTo(unchanged));
            client.Command("unequip", shieldId, 2);
            yield return Until(() => client.State.inventory.backpack[0].id == shieldId, "unequip");
            client.Command("take", incoming, 0);
            yield return Until(() => client.State.inventory.backpack[0].id == incoming, "swap with chest");
            Assert.That(Array.Find(client.State.inventory.chest, item => item.id == shieldId).shield, Is.EqualTo(100));
            Assert.That(client.State.elapsed, Is.EqualTo(chestTime));
            yield return EquipFromChest(client, "Power Shooter", 0, 0);
            yield return EquipFromChest(client, "Quick Shooter", 1, 1);
            yield return EquipFromChest(client, "Shield", 2, 2);
            yield return EquipFromChest(client, "Grenade", 1, 3);
            int grenadeId = client.State.inventory.equipment[3].id;
            // Real Input System presses: a held UI key must be released and pressed again.
            var driver = ConfigureInput(client);
            driver.KeyboardState = new KeyboardState(Key.X);
            yield return null; yield return null;
            client.Command("close");
            yield return Until(() => client.State.overlay == "none" && client.State.elapsed > chestTime, "close resumes game");
            tick = client.State.tick;
            yield return Until(() => client.State.tick > tick + 5, "held-key release gate");
            Assert.That(client.State.inventory.equipment[3].id, Is.EqualTo(grenadeId));
            driver.KeyboardState = new KeyboardState(); yield return null; yield return null;
            driver.KeyboardState = new KeyboardState(Key.X);
            yield return Until(() => client.State.inventory.equipment[3].id == 0, "fresh press consumes grenade");
            Assert.That(client.State.grenades.Length, Is.EqualTo(1));
            client.Command("pause");
            yield return Until(() => client.State.overlay == "pause", "pause grenade flight");
            float flight = client.State.grenades[0].Remaining;
            tick = client.State.tick;
            yield return Until(() => client.State.tick > tick + 5, "paused grenade clock");
            Assert.That(client.State.grenades[0].Remaining, Is.EqualTo(flight));
            client.DeviceInput = false; driver.enabled = false;
            client.Command("resume");
            yield return Until(() => client.State.overlay == "none", "resume flight");
            deadline = Time.realtimeSinceStartup + 10f;
            while (client.State.phase == 1 && Time.realtimeSinceStartup < deadline)
            {
                client.Frame((ArenaSimulation.PortalPosition - client.State.playerPosition).normalized, ArenaSimulation.PortalPosition);
                yield return null;
            }
            yield return Until(() => client.State.phase == 3, "enter first combat floor");
            Assert.That(client.State.enemies.Length, Is.EqualTo(3));
            foreach (var enemy in client.State.enemies)
                Assert.That(root.transform.Find("Arena presentation/enemy-" + enemy.Id), Is.Not.Null);
            deadline = Time.realtimeSinceStartup + 12f;
            while (client.State.phase == 3 && Time.realtimeSinceStartup < deadline)
            {
                var enemy = client.State.enemies.OrderBy(e => (e.Position - client.State.playerPosition).sqrMagnitude).First();
                client.Frame(Vector2.zero, enemy.Position, true, true, true);
                yield return null;
            }
            Assert.That(client.State.phase, Is.EqualTo(4), "live authoritative combat clears the first floor");
            Assert.That(client.State.enemiesDefeated, Is.EqualTo(3));
            Assert.That(client.State.floorsCleared, Is.EqualTo(1));
            Assert.That(client.State.inventory.health, Is.GreaterThan(0));
            Assert.That(client.State.projectiles, Is.Empty);
            Assert.That(client.State.grenades, Is.Empty);
            Assert.That(client.State.effects, Is.Empty);
            yield return null;
        }

        private static IEnumerator EquipFromChest(KituArenaClient client, string name, int bag, int slot)
        {
            int id = Array.Find(client.State.inventory.chest, item => item.name == name).id;
            client.Command("take", id, bag);
            yield return Until(() => client.State.inventory.backpack[bag].id == id, "take " + name);
            client.Command("equip", id, bag, slot);
            yield return Until(() => client.State.inventory.equipment[slot].id == id, "equip " + name);
        }

        private ArenaFrontendInputDriver ConfigureInput(KituArenaClient client)
        {
            previousKeyboard = Keyboard.current; previousMouse = Mouse.current;
            previousUpdateMode = InputSystem.settings.updateMode;
            previousBackgroundBehavior = InputSystem.settings.backgroundBehavior;
            previousEditorInput = InputSystem.settings.editorInputBehaviorInPlayMode;
            previousBackground = Application.runInBackground;
            inputConfigured = true;
            Application.runInBackground = true;
            InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
            InputSystem.settings.editorInputBehaviorInPlayMode = InputSettings.EditorInputBehaviorInPlayMode.AllDeviceInputAlwaysGoesToGameView;
            InputSystem.settings.updateMode = InputSettings.UpdateMode.ProcessEventsManually;
            keyboard = InputSystem.AddDevice<Keyboard>("Kitu Arena test keyboard");
            mouse = InputSystem.AddDevice<Mouse>("Kitu Arena test mouse");
            var driver = root.AddComponent<ArenaFrontendInputDriver>();
            driver.Keyboard = keyboard; driver.Mouse = mouse;
            driver.MouseState = new MouseState { position = client.GameCamera.WorldToScreenPoint(ArenaWorldView.Point(client.State.playerPosition + Vector2.up * 3f, 0)) };
            client.DeviceInput = true;
            return driver;
        }

        private static IEnumerator Until(Func<bool> condition, string operation)
        {
            float deadline = Time.realtimeSinceStartup + 12f;
            while (!condition() && Time.realtimeSinceStartup < deadline) yield return null;
            Assert.That(condition(), Is.True, operation);
        }
    }
}
