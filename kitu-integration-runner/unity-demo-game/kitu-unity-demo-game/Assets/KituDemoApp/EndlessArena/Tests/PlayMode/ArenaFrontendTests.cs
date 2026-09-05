using System;
using System.Collections;
using System.Linq;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;
using UnityEngine.TestTools;

namespace UnityOnlyArena.Tests
{
    // These tests exercise ArenaGame.Update with real Input System state events. They do
    // not assert visual readability, an OS window mode change or an actual application quit.
    public sealed class ArenaFrontendTests
    {
        private ArenaGame game;
        private GameObject root;
        private ArenaFrontendInputDriver inputDriver;
        private Keyboard keyboard, previousKeyboard;
        private Mouse mouse, previousMouse;
        private InputSettings.UpdateMode previousUpdateMode;
        private InputSettings.BackgroundBehavior previousBackgroundBehavior;
        private InputSettings.EditorInputBehaviorInPlayMode previousEditorInputBehavior;
        private bool previousRunInBackground;
        private float previousAudioVolume, previousSavedVolume;
        private int previousSavedFullscreen;
        private bool hadVolumePreference, hadFullscreenPreference, previousCursorVisible;
        private CursorLockMode previousCursorLock;
        private Color previousAmbient;
        private string isolatedPrefix;
        private static readonly Key[] NoKeys = Array.Empty<Key>();
        private static readonly Vector2 AimTarget = new Vector2(4f, -3f);

        [UnitySetUp]
        public IEnumerator SetUp()
        {
            previousAudioVolume = AudioListener.volume;
            previousCursorVisible = Cursor.visible;
            previousCursorLock = Cursor.lockState;
            previousAmbient = RenderSettings.ambientLight;
            hadVolumePreference = PlayerPrefs.HasKey(ArenaSettings.PreferencePrefix + "Volume");
            hadFullscreenPreference = PlayerPrefs.HasKey(ArenaSettings.PreferencePrefix + "Fullscreen");
            previousSavedVolume = PlayerPrefs.GetFloat(ArenaSettings.PreferencePrefix + "Volume", 1f);
            previousSavedFullscreen = PlayerPrefs.GetInt(ArenaSettings.PreferencePrefix + "Fullscreen", 0);
            isolatedPrefix = "UnityOnlyArena.Tests." + Guid.NewGuid().ToString("N") + ".";

            previousKeyboard = Keyboard.current;
            previousMouse = Mouse.current;
            previousUpdateMode = InputSystem.settings.updateMode;
            previousBackgroundBehavior = InputSystem.settings.backgroundBehavior;
            previousEditorInputBehavior = InputSystem.settings.editorInputBehaviorInPlayMode;
            previousRunInBackground = Application.runInBackground;
            Application.runInBackground = true;
            InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
            InputSystem.settings.editorInputBehaviorInPlayMode = InputSettings.EditorInputBehaviorInPlayMode.AllDeviceInputAlwaysGoesToGameView;
            InputSystem.settings.updateMode = InputSettings.UpdateMode.ProcessEventsManually;
            keyboard = InputSystem.AddDevice<Keyboard>("Arena test keyboard");
            mouse = InputSystem.AddDevice<Mouse>("Arena test mouse");
            root = new GameObject("Arena frontend test");
            inputDriver = root.AddComponent<ArenaFrontendInputDriver>();
            inputDriver.Keyboard = keyboard;
            inputDriver.Mouse = mouse;
            game = root.AddComponent<ArenaGame>();
            yield return InputFrame(NoKeys);
        }

        [UnityTearDown]
        public IEnumerator TearDown()
        {
            if (root != null)
            {
                root.SetActive(false);
                UnityEngine.Object.Destroy(root);
            }
            if (keyboard != null && keyboard.added) InputSystem.RemoveDevice(keyboard);
            if (mouse != null && mouse.added) InputSystem.RemoveDevice(mouse);
            InputSystem.settings.updateMode = previousUpdateMode;
            InputSystem.settings.backgroundBehavior = previousBackgroundBehavior;
            InputSystem.settings.editorInputBehaviorInPlayMode = previousEditorInputBehavior;
            Application.runInBackground = previousRunInBackground;
            if (previousKeyboard != null && previousKeyboard.added) previousKeyboard.MakeCurrent();
            if (previousMouse != null && previousMouse.added) previousMouse.MakeCurrent();
            RestorePreference("Volume", hadVolumePreference, previousSavedVolume);
            string fullscreenKey = ArenaSettings.PreferencePrefix + "Fullscreen";
            if (hadFullscreenPreference) PlayerPrefs.SetInt(fullscreenKey, previousSavedFullscreen);
            else PlayerPrefs.DeleteKey(fullscreenKey);
            if (isolatedPrefix != null)
            {
                PlayerPrefs.DeleteKey(isolatedPrefix + "Volume");
                PlayerPrefs.DeleteKey(isolatedPrefix + "Fullscreen");
            }
            PlayerPrefs.Save();
            AudioListener.volume = previousAudioVolume;
            Cursor.visible = previousCursorVisible;
            Cursor.lockState = previousCursorLock;
            RenderSettings.ambientLight = previousAmbient;
            yield return null;
        }

        [UnityTest]
        public IEnumerator OpeningStartAndReturnToMenuOwnTheSceneAndRunLifetime()
        {
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
            Assert.That(game.IsSimulationRunning, Is.False);
            game.AdvanceSimulation(.1f, new ArenaInput { Move = Vector2.up, FireA = true });
            Assert.That(game.Model.Elapsed, Is.Zero);
            Assert.That(game.Model.Enemies, Is.Empty);
            Assert.That(game.GameCamera, Is.Not.Null);
            Assert.That(game.GameCamera.orthographic, Is.True);
            Assert.That(root.GetComponent<ArenaHud>(), Is.Not.Null);

            game.StartRun();
            yield return InputFrame(NoKeys);
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Preparing));
            Assert.That(game.Model.Floor, Is.Zero);
            Assert.That(game.Model.ChestAvailable, Is.True);
            Assert.That(game.Model.PortalAvailable, Is.True);
            Assert.That(game.Model.Enemies, Is.Empty);
            Assert.That(root.transform.Find("Arena presentation/Player").gameObject.activeInHierarchy, Is.True);
            Assert.That(root.transform.Find("Arena presentation/Supply chest").gameObject.activeInHierarchy, Is.True);

            EquipFromChest(ItemKind.Shield, EquipmentSlot.ItemA);
            game.Model.Inventory.ApplyDamage(25);
            game.SetPaused(true);
            game.ReturnToMenu();
            yield return InputFrame(NoKeys);
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            Assert.That(game.Model.Elapsed, Is.Zero);
            Assert.That(game.Model.Inventory.Chest, Is.Empty);
            Assert.That(game.Model.Inventory.Equipment.All(item => item == null), Is.True);
            Assert.That(root.transform.Find("Arena presentation").gameObject.activeSelf, Is.False);

            game.StartRun();
            Assert.That(game.Model.Inventory.Health, Is.EqualTo(100));
            Assert.That(game.Model.Inventory.Equipment[0].Kind, Is.EqualTo(ItemKind.Blade));
            Assert.That(game.Model.Inventory.Equipment.Skip(1).All(item => item == null), Is.True);
            Assert.That(game.Model.Inventory.ShieldDelayRemaining, Is.Zero);
            Assert.That(game.Model.Inventory.Chest.Count, Is.EqualTo(11));
        }

        [UnityTest]
        public IEnumerator SettingsDraftCancelResetAndApplyReturnToOpeningWithoutAdvancingRun()
        {
            float originalVolume = game.Settings.Volume;
            bool originalFullscreen = game.Settings.Fullscreen;
            game.OpenSettings();
            game.DraftSettings.Volume = .23f;
            game.DraftSettings.Fullscreen = !originalFullscreen;
            Assert.That(game.Settings.Volume, Is.EqualTo(originalVolume));
            Assert.That(AudioListener.volume, Is.EqualTo(originalVolume).Within(.0001f));
            game.ResetSettingsDraft();
            Assert.That(game.DraftSettings.Volume, Is.EqualTo(1f));
            Assert.That(game.DraftSettings.Fullscreen, Is.False);
            yield return InputFrame(new[] { Key.Escape });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
            Assert.That(game.Settings.Volume, Is.EqualTo(originalVolume));
            Assert.That(game.Settings.Fullscreen, Is.EqualTo(originalFullscreen));
            Assert.That(game.Model.Elapsed, Is.Zero);

            yield return InputFrame(NoKeys);
            game.OpenSettings();
            game.DraftSettings.Volume = .37f;
            game.DraftSettings.Fullscreen = !originalFullscreen;
            game.ApplySettings();
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
            Assert.That(AudioListener.volume, Is.EqualTo(.37f).Within(.0001f));
            ArenaSettings saved = ArenaSettings.Load();
            Assert.That(saved.Volume, Is.EqualTo(.37f).Within(.0001f));
            Assert.That(saved.Fullscreen, Is.EqualTo(!originalFullscreen));
            Assert.That(game.Model.Elapsed, Is.Zero);
        }

        [UnityTest]
        public IEnumerator PauseSettingsAndFocusLossFreezeCombatAndReturnToPause()
        {
            game.StartRun();
            EquipFromChest(ItemKind.Shield, EquipmentSlot.ItemA);
            EquipFromChest(ItemKind.Grenade, EquipmentSlot.ItemB);
            Assert.That(game.Model.TryAdvanceFloor(), Is.True);
            for (int i = 0; i < 4; i++) game.AdvanceSimulation(.1f, default);
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Combat));
            game.Model.Inventory.ApplyDamage(30);
            game.AdvanceSimulation(1f / 60f, new ArenaInput
            {
                HasAim = true, AimPoint = AimTarget, UseB = true, FireA = true
            });
            Assert.That(game.Model.Grenades.Count, Is.EqualTo(1));
            yield return InputFrame(new[] { Key.Escape });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Pause));
            var paused = new FrozenState(game.Model);

            game.OpenSettings();
            game.DraftSettings.Volume = .14f;
            game.AdvanceSimulation(.1f, new ArenaInput { Move = Vector2.up, FireA = true, UseA = true });
            yield return InputFrame(new[] { Key.W, Key.Z, Key.X }, true, true);
            paused.AssertUnchanged(game.Model);
            yield return InputFrame(new[] { Key.Escape });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Pause));
            paused.AssertUnchanged(game.Model);

            yield return InputFrame(NoKeys);
            game.OpenSettings();
            game.DraftSettings.Volume = .41f;
            game.ApplySettings();
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Pause));
            game.AdvanceSimulation(.1f, default);
            yield return InputFrame(NoKeys);
            paused.AssertUnchanged(game.Model);

            yield return InputFrame(new[] { Key.Escape });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            game.SendMessage("OnApplicationFocus", false);
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Pause));
            game.SendMessage("OnApplicationFocus", true);
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Pause), "Focus restoration must not resume combat.");
        }

        [UnityTest]
        public IEnumerator ZAndXUseTheirOwnSlotsThroughTheInputSystem()
        {
            foreach (bool reverse in new[] { false, true })
            {
                game.ReturnToMenu();
                game.StartRun();
                EquipmentSlot medkitSlot = reverse ? EquipmentSlot.ItemB : EquipmentSlot.ItemA;
                EquipmentSlot grenadeSlot = reverse ? EquipmentSlot.ItemA : EquipmentSlot.ItemB;
                EquipFromChest(ItemKind.Medkit, medkitSlot);
                EquipFromChest(ItemKind.Grenade, grenadeSlot);
                game.Model.Inventory.ApplyDamage(40);
                yield return InputFrame(NoKeys);
                Key medkitKey = reverse ? Key.X : Key.Z;
                yield return InputUntil(() => game.Model.Inventory.Equipment[(int)medkitSlot] == null,
                    new[] { medkitKey });
                Assert.That(game.Model.Inventory.Health, Is.EqualTo(100));
                Assert.That(game.Model.Inventory.Equipment[(int)grenadeSlot].Kind, Is.EqualTo(ItemKind.Grenade));

                yield return InputFrame(NoKeys);
                Key grenadeKey = reverse ? Key.Z : Key.X;
                yield return InputUntil(() => game.Model.Inventory.Equipment[(int)grenadeSlot] == null,
                    new[] { grenadeKey });
                Assert.That(game.Model.Grenades.Count, Is.EqualTo(1));
                Assert.That(game.Model.Grenades[0].Damage, Is.EqualTo(100));
                Assert.That(Vector2.Distance(game.Model.Grenades[0].Target, AimTarget), Is.LessThan(.01f));
            }
        }

        [UnityTest]
        public IEnumerator FullHealthMedkitAndSimultaneousMedkitsDoNotConsumeAnExtraItem()
        {
            game.StartRun();
            ArenaItem medkitA = EquipFromChest(ItemKind.Medkit, EquipmentSlot.ItemA);
            yield return InputFrame(NoKeys);
            float elapsed = game.Model.Elapsed;
            yield return InputUntil(() => game.Model.Elapsed > elapsed + .04f, new[] { Key.Z });
            Assert.That(game.Model.Inventory.Equipment[(int)EquipmentSlot.ItemA], Is.SameAs(medkitA));
            Assert.That(game.Model.Inventory.Health, Is.EqualTo(100));

            // Obtain a distinct second item through the same inventory transfer API.
            game.Model.Inventory.CreateChest(5);
            ArenaItem medkitB = EquipFromChest(ItemKind.Medkit, EquipmentSlot.ItemB);
            game.Model.Inventory.ApplyDamage(40);
            yield return InputFrame(NoKeys);
            yield return InputUntil(() => game.Model.Inventory.Equipment[(int)EquipmentSlot.ItemA] == null,
                new[] { Key.Z, Key.X });
            Assert.That(game.Model.Inventory.Health, Is.EqualTo(100));
            Assert.That(game.Model.Inventory.Equipment[(int)EquipmentSlot.ItemB], Is.SameAs(medkitB));
        }

        [UnityTest]
        public IEnumerator HeldButtonsAcrossInventoryCloseRequireReleaseBeforeWeaponsOrItems()
        {
            game.StartRun();
            EquipFromChest(ItemKind.Shooter, EquipmentSlot.WeaponB);
            ArenaItem medkit = EquipFromChest(ItemKind.Medkit, EquipmentSlot.ItemA);
            ArenaItem grenade = EquipFromChest(ItemKind.Grenade, EquipmentSlot.ItemB);
            game.Model.Inventory.ApplyDamage(40);
            yield return InputFrame(NoKeys);
            yield return InputFrame(new[] { Key.Tab, Key.Z, Key.X }, true, true);
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Inventory));
            var frozen = new FrozenState(game.Model);
            yield return InputFrame(new[] { Key.Z, Key.X }, true, true);
            frozen.AssertUnchanged(game.Model);
            yield return InputFrame(new[] { Key.Tab, Key.Z, Key.X }, true, true);
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            float elapsed = game.Model.Elapsed;
            yield return InputUntil(() => game.Model.Elapsed > elapsed + .04f, new[] { Key.Z, Key.X }, true, true);
            Assert.That(game.Model.Inventory.Health, Is.EqualTo(60));
            Assert.That(game.Model.Inventory.Equipment[2], Is.SameAs(medkit));
            Assert.That(game.Model.Inventory.Equipment[3], Is.SameAs(grenade));
            Assert.That(game.Model.WeaponCooldowns, Is.All.EqualTo(0f));
            Assert.That(game.Model.Projectiles, Is.Empty);
            Assert.That(game.Model.Grenades, Is.Empty);

            yield return InputFrame(NoKeys);
            yield return InputUntil(() => game.Model.Inventory.Equipment[2] == null && game.Model.Inventory.Equipment[3] == null,
                new[] { Key.Z, Key.X }, true, true);
            Assert.That(game.Model.Inventory.Health, Is.EqualTo(100));
            Assert.That(game.Model.WeaponCooldowns[0], Is.GreaterThan(0f));
            Assert.That(game.Model.WeaponCooldowns[1], Is.GreaterThan(0f));
            Assert.That(game.Model.Projectiles.Count, Is.GreaterThanOrEqualTo(1));
            Assert.That(game.Model.Grenades.Count, Is.EqualTo(1));
        }

        [UnityTest]
        public IEnumerator WasdAimAndBothMouseButtonsReachTheRunningSimulation()
        {
            game.StartRun();
            EquipFromChest(ItemKind.Shooter, EquipmentSlot.WeaponA);
            EquipFromChest(ItemKind.HeavyShooter, EquipmentSlot.WeaponB);
            yield return InputFrame(NoKeys);
            Vector2 initial = game.Model.PlayerPosition;
            yield return InputUntil(() => game.Model.PlayerPosition.x > initial.x && game.Model.PlayerPosition.y > initial.y &&
                game.Model.WeaponCooldowns[0] > 0f && game.Model.WeaponCooldowns[1] > 0f,
                new[] { Key.W, Key.D }, true, true);
            Vector2 moved = game.Model.PlayerPosition - initial;
            Assert.That(moved.x, Is.EqualTo(moved.y).Within(.001f));
            Assert.That(Vector2.Dot(game.Model.AimDirection, (AimTarget - game.Model.PlayerPosition).normalized), Is.GreaterThan(.999f));
            Assert.That(game.Model.Projectiles.Any(shot => shot.Damage == 12 && !shot.EnemyOwned), Is.True);
            Assert.That(game.Model.Projectiles.Any(shot => shot.Damage == 50 && !shot.EnemyOwned), Is.True);

            yield return InputFrame(NoKeys);
            Vector2 beforeReturn = game.Model.PlayerPosition;
            yield return InputUntil(() => game.Model.PlayerPosition.x < beforeReturn.x && game.Model.PlayerPosition.y < beforeReturn.y,
                new[] { Key.A, Key.S });
            Assert.That(game.Model.PlayerPosition.x, Is.LessThan(beforeReturn.x));
            Assert.That(game.Model.PlayerPosition.y, Is.LessThan(beforeReturn.y));
        }

        [UnityTest]
        public IEnumerator ChestDistanceAndCombatInventoryGatesUseEAndTab()
        {
            game.StartRun();
            yield return InputFrame(new[] { Key.E });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            StringAssert.Contains("within 2 units", game.Message);
            game.Model.PlayerPosition = ArenaSimulation.ChestPosition;
            yield return InputFrame(NoKeys);
            yield return InputFrame(new[] { Key.E });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Chest));
            yield return InputFrame(new[] { Key.Escape });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            Assert.That(game.Model.TryAdvanceFloor(), Is.True);
            for (int i = 0; i < 4; i++) game.AdvanceSimulation(.1f, default);
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Combat));
            yield return InputFrame(new[] { Key.Tab });
            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
            StringAssert.Contains("after clearing", game.Message);
        }

        [Test]
        public void SettingsSaveLoadUsesAnIsolatedPrefixAndRejectsInvalidSavedValues()
        {
            ArenaSettings initial = ArenaSettings.Load(isolatedPrefix);
            Assert.That(initial.Volume, Is.EqualTo(1f));
            Assert.That(initial.Fullscreen, Is.False);
            var settings = new ArenaSettings { Volume = .28f, Fullscreen = true };
            settings.Apply(true, isolatedPrefix);
            ArenaSettings loaded = ArenaSettings.Load(isolatedPrefix);
            Assert.That(loaded.Volume, Is.EqualTo(.28f).Within(.0001f));
            Assert.That(loaded.Fullscreen, Is.True);
            Assert.That(AudioListener.volume, Is.EqualTo(.28f).Within(.0001f));
            loaded.Copy().Volume = .8f;
            Assert.That(ArenaSettings.Load(isolatedPrefix).Volume, Is.EqualTo(.28f).Within(.0001f));

            PlayerPrefs.SetFloat(isolatedPrefix + "Volume", 3f);
            PlayerPrefs.SetInt(isolatedPrefix + "Fullscreen", 7);
            ArenaSettings invalid = ArenaSettings.Load(isolatedPrefix);
            Assert.That(invalid.Volume, Is.EqualTo(1f));
            Assert.That(invalid.Fullscreen, Is.False);
            settings.Volume = 0f;
            settings.Apply(true, isolatedPrefix);
            Assert.That(AudioListener.volume, Is.Zero);
        }

        private ArenaItem EquipFromChest(ItemKind kind, EquipmentSlot slot)
        {
            ArenaInventory inventory = game.Model.Inventory;
            int chestIndex = inventory.Chest.FindIndex(item => item.Kind == kind);
            int backpackIndex = Array.FindIndex(inventory.Backpack, item => item == null);
            Assert.That(chestIndex, Is.GreaterThanOrEqualTo(0), "Missing fixture chest item: " + kind);
            Assert.That(backpackIndex, Is.GreaterThanOrEqualTo(0), "Fixture needs an empty backpack slot.");
            Assert.That(inventory.TakeChest(chestIndex, backpackIndex), Is.True);
            Assert.That(inventory.Equip(backpackIndex, slot), Is.True);
            return inventory.Equipment[(int)slot];
        }

        private IEnumerator InputFrame(Key[] keys, bool left = false, bool right = false)
        {
            Vector3 screen = game.GameCamera.WorldToScreenPoint(new Vector3(AimTarget.x, 0f, AimTarget.y));
            var pointer = new MouseState { position = new Vector2(screen.x, screen.y) }
                .WithButton(MouseButton.Left, left).WithButton(MouseButton.Right, right);
            inputDriver.KeyboardState = new KeyboardState(keys);
            inputDriver.MouseState = pointer;
            int request = ++inputDriver.RequestedFrame;
            double deadline = Time.realtimeSinceStartupAsDouble + 5;
            // The early Update processes input; LateUpdate proves ArenaGame.Update had its
            // opportunity to consume it. A coroutine continuation alone does not prove that.
            while (inputDriver.CompletedFrame < request && Time.realtimeSinceStartupAsDouble < deadline)
                yield return null;
            Assert.That(inputDriver.CompletedFrame, Is.GreaterThanOrEqualTo(request),
                "The input driver did not complete a real Unity Update/LateUpdate frame.");
        }

        private IEnumerator InputUntil(Func<bool> condition, Key[] keys, bool left = false, bool right = false)
        {
            // Batch tests are unthrottled: hundreds of frames can pass before the game's
            // 1/60-second step. Bound wall time instead of assuming a minimum frame time.
            double deadline = Time.realtimeSinceStartupAsDouble + 5;
            while (!condition() && Time.realtimeSinceStartupAsDouble < deadline)
                yield return InputFrame(keys, left, right);
            Assert.That(condition(), Is.True, "Input did not reach the expected game state within five seconds.");
        }

        private static void RestorePreference(string suffix, bool existed, float value)
        {
            string key = ArenaSettings.PreferencePrefix + suffix;
            if (existed) PlayerPrefs.SetFloat(key, value);
            else PlayerPrefs.DeleteKey(key);
        }

        private sealed class FrozenState
        {
            private readonly float elapsed, shieldDelay;
            private readonly int health, shield;
            private readonly Vector2 player;
            private readonly Vector2[] enemies, projectiles;
            private readonly float[] cooldowns, grenades;

            public FrozenState(ArenaSimulation model)
            {
                elapsed = model.Elapsed;
                health = model.Inventory.Health;
                shieldDelay = model.Inventory.ShieldDelayRemaining;
                shield = model.Inventory.Equipment.Where(item => item != null && item.Kind == ItemKind.Shield).Sum(item => item.Shield);
                player = model.PlayerPosition;
                enemies = model.Enemies.Select(enemy => enemy.Position).ToArray();
                projectiles = model.Projectiles.Select(shot => shot.Position).ToArray();
                cooldowns = model.WeaponCooldowns.ToArray();
                grenades = model.Grenades.Select(grenade => grenade.Remaining).ToArray();
            }

            public void AssertUnchanged(ArenaSimulation model)
            {
                Assert.That(model.Elapsed, Is.EqualTo(elapsed));
                Assert.That(model.Inventory.Health, Is.EqualTo(health));
                Assert.That(model.Inventory.ShieldDelayRemaining, Is.EqualTo(shieldDelay));
                Assert.That(model.Inventory.Equipment.Where(item => item != null && item.Kind == ItemKind.Shield).Sum(item => item.Shield), Is.EqualTo(shield));
                Assert.That(model.PlayerPosition, Is.EqualTo(player));
                CollectionAssert.AreEqual(enemies, model.Enemies.Select(enemy => enemy.Position).ToArray());
                CollectionAssert.AreEqual(projectiles, model.Projectiles.Select(shot => shot.Position).ToArray());
                CollectionAssert.AreEqual(cooldowns, model.WeaponCooldowns);
                CollectionAssert.AreEqual(grenades, model.Grenades.Select(grenade => grenade.Remaining).ToArray());
            }
        }
    }

    [DefaultExecutionOrder(-10000)]
    public sealed class ArenaFrontendInputDriver : MonoBehaviour
    {
        public Keyboard Keyboard;
        public Mouse Mouse;
        public KeyboardState KeyboardState;
        public MouseState MouseState;
        public int RequestedFrame;
        public int CompletedFrame { get; private set; }
        private int processedFrame;

        private void Update()
        {
            InputSystem.QueueStateEvent(Keyboard, KeyboardState);
            InputSystem.QueueStateEvent(Mouse, MouseState);
            InputSystem.Update();
            Keyboard.MakeCurrent();
            Mouse.MakeCurrent();
            processedFrame = RequestedFrame;
        }

        private void LateUpdate() => CompletedFrame = processedFrame;
    }
}
