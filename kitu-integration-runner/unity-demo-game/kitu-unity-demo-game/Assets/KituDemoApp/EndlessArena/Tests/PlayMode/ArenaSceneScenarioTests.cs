using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.InputSystem.LowLevel;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaSceneScenarioTests
    {
        private const string ScenePath = "Assets/KituDemoApp/EndlessArena/EndlessArena.unity";
        private const float Tick = 1f / 60f;
        private Scene loadedScene, previousScene;
        private ArenaGame game;
        private Keyboard keyboard, previousKeyboard;
        private Mouse mouse, previousMouse;
        private ArenaFrontendInputDriver neutralInput;
        private InputSettings.UpdateMode updateMode;
        private InputSettings.BackgroundBehavior background;
        private InputSettings.EditorInputBehaviorInPlayMode editorInput;
        private bool runInBackground, cursorVisible;
        private CursorLockMode cursorLock;
        private float audioVolume;
        private Color ambient;
        private string visualDirectory;

        [UnitySetUp]
        public IEnumerator LoadActualScene()
        {
            previousScene = SceneManager.GetActiveScene();
            audioVolume = AudioListener.volume;
            ambient = RenderSettings.ambientLight;
            cursorVisible = Cursor.visible;
            cursorLock = Cursor.lockState;
            runInBackground = Application.runInBackground;
            updateMode = InputSystem.settings.updateMode;
            background = InputSystem.settings.backgroundBehavior;
            editorInput = InputSystem.settings.editorInputBehaviorInPlayMode;
            previousKeyboard = Keyboard.current;
            previousMouse = Mouse.current;
            Application.runInBackground = true;
            InputSystem.settings.backgroundBehavior = InputSettings.BackgroundBehavior.IgnoreFocus;
            InputSystem.settings.editorInputBehaviorInPlayMode = InputSettings.EditorInputBehaviorInPlayMode.AllDeviceInputAlwaysGoesToGameView;
            InputSystem.settings.updateMode = InputSettings.UpdateMode.ProcessEventsManually;
            keyboard = InputSystem.AddDevice<Keyboard>("Arena scene neutral keyboard");
            mouse = InputSystem.AddDevice<Mouse>("Arena scene neutral mouse");

            yield return SceneManager.LoadSceneAsync(ScenePath, LoadSceneMode.Additive);
            loadedScene = SceneManager.GetSceneByPath(ScenePath);
            Assert.That(loadedScene.isLoaded, Is.True);
            SceneManager.SetActiveScene(loadedScene);
            ArenaGame[] games = loadedScene.GetRootGameObjects().SelectMany(root => root.GetComponentsInChildren<ArenaGame>()).ToArray();
            Assert.That(games.Length, Is.EqualTo(1), "The saved scene must supply exactly one real game controller.");
            game = games[0];
            neutralInput = new GameObject("Scene scenario neutral input").AddComponent<ArenaFrontendInputDriver>();
            neutralInput.Keyboard = keyboard;
            neutralInput.Mouse = mouse;
            neutralInput.KeyboardState = new KeyboardState(Array.Empty<Key>());
            neutralInput.MouseState = new MouseState { position = new Vector2(Screen.width / 2f, Screen.height / 2f) };
            visualDirectory = Path.GetFullPath(Path.Combine(Application.dataPath, "..", "Logs", "arena-visuals"));
            Directory.CreateDirectory(visualDirectory);
            yield return null;
            yield return null;
        }

        [UnityTearDown]
        public IEnumerator RestoreSceneAndDeviceState()
        {
            if (previousScene.IsValid() && previousScene.isLoaded) SceneManager.SetActiveScene(previousScene);
            if (loadedScene.IsValid() && loadedScene.isLoaded) yield return SceneManager.UnloadSceneAsync(loadedScene);
            if (keyboard != null && keyboard.added) InputSystem.RemoveDevice(keyboard);
            if (mouse != null && mouse.added) InputSystem.RemoveDevice(mouse);
            InputSystem.settings.updateMode = updateMode;
            InputSystem.settings.backgroundBehavior = background;
            InputSystem.settings.editorInputBehaviorInPlayMode = editorInput;
            Application.runInBackground = runInBackground;
            if (previousKeyboard != null && previousKeyboard.added) previousKeyboard.MakeCurrent();
            if (previousMouse != null && previousMouse.added) previousMouse.MakeCurrent();
            AudioListener.volume = audioVolume;
            RenderSettings.ambientLight = ambient;
            Cursor.visible = cursorVisible;
            Cursor.lockState = cursorLock;
        }

        [UnityTest]
        public IEnumerator ActualSceneStockRunReachesElevenThenDiesAndRestarts()
        {
            Debug.Log("[scene run] Actual saved scene; ArenaGame.AdvanceSimulation accelerates the stock input bot. " +
                "ArenaGame.Update, WorldView.LateUpdate and HUD.OnGUI remain enabled. Physical input is covered separately.");
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
            AssertPresentationMatches();
            game.Quit();
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Opening));
            StringAssert.Contains("Editor remains open", game.Message);
            yield return CaptureRenderedFrame("opening");
            game.StartRun();
            var enteredFloors = new HashSet<int>();
            var preparedFloors = new HashSet<int>();
            int preparedFloor = -1, observedClears = 0;
            bool exitPortal = false, capturedChest = false, capturedCombat = false, capturedBoss = false;
            // At most twenty minutes of ordinary game time, with rendering opportunities
            // between bounded batches. No actor position, HP, attack stat or enemy is edited.
            for (int tick = 0; tick < 60 * 60 * 20; tick++)
            {
                ArenaSimulation model = game.Model;
                Assert.That(model.Phase, Is.Not.EqualTo(ArenaPhase.Results), "Stock bot died before 11F: " + Checkpoint());
                Assert.That(game.enabled && game.isActiveAndEnabled, Is.True);
                Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.None));
                if (model.Phase == ArenaPhase.Combat && enteredFloors.Add(model.Floor))
                {
                    Debug.Log("[scene run] entered " + Checkpoint());
                    yield return null;
                    yield return null;
                    AssertPresentationMatches();
                }
                if (model.Floor >= 11 && model.Phase == ArenaPhase.Combat) break;
                if (model.FloorsCleared != observedClears)
                {
                    observedClears = model.FloorsCleared;
                    exitPortal = Vector2.Distance(model.PlayerPosition, ArenaSimulation.PortalPosition) <= ArenaSimulation.PortalRadius;
                    Debug.Log("[scene run] cleared " + Checkpoint());
                }
                if (!capturedCombat && model.Floor == 1 && model.Phase == ArenaPhase.Combat)
                {
                    yield return CaptureRenderedFrame("combat");
                    capturedCombat = true;
                }
                if (!capturedBoss && model.IsBossFloor && model.Enemies.Any(enemy => enemy.BossState == ArenaBossState.Telegraph))
                {
                    yield return CaptureRenderedFrame("boss");
                    capturedBoss = true;
                }

                var input = new ArenaInput();
                if (model.IsSafe)
                {
                    if (exitPortal)
                    {
                        input.Move = Vector2.down;
                        if (Vector2.Distance(model.PlayerPosition, ArenaSimulation.PortalPosition) > ArenaSimulation.PortalRadius + .25f)
                            exitPortal = false;
                    }
                    else if (model.ChestAvailable && preparedFloor != model.Floor)
                    {
                        Vector2 offset = ArenaSimulation.ChestPosition - model.PlayerPosition;
                        if (offset.magnitude > 1.5f) input.Move = MoveToChestAvoidingPortal(model.PlayerPosition);
                        else
                        {
                            game.OpenChest();
                            Assert.That(game.Overlay, Is.EqualTo(ArenaOverlay.Chest));
                            SelectStockSupplies();
                            if (!capturedChest)
                            {
                                yield return CaptureRenderedFrame("chest");
                                capturedChest = true;
                            }
                            game.CloseOverlay();
                            preparedFloor = model.Floor;
                            Assert.That(preparedFloors.Add(preparedFloor), Is.True, "A reward chest was prepared twice.");
                            Debug.Log("[scene run] selected stock chest " + Checkpoint());
                        }
                    }
                    else input.Move = (ArenaSimulation.PortalPosition - model.PlayerPosition).normalized;
                }
                else if (model.Phase == ArenaPhase.Combat)
                {
                    float radius = model.PlayerPosition.magnitude;
                    Vector2 radial = radius > .01f ? model.PlayerPosition / radius : Vector2.down;
                    Vector2 tangent = new Vector2(-radial.y, radial.x);
                    input.Move = (tangent + radial * ((7.7f - radius) * 1.5f)).normalized;
                    input.HasAim = true;
                    input.AimPoint = model.Enemies.OrderBy(enemy => (enemy.Position - model.PlayerPosition).sqrMagnitude).First().Position;
                    input.FireA = input.FireB = true;
                    input.UseB = model.Inventory.Health <= model.Inventory.MaxHealth / 2;
                    Vector3 screen = game.GameCamera.WorldToScreenPoint(ArenaWorldView.Point(input.AimPoint, 0));
                    neutralInput.MouseState = new MouseState { position = new Vector2(screen.x, screen.y) };
                }
                game.AdvanceSimulation(Tick, input);
                if (tick % 120 == 0) yield return null;
            }
            CollectionAssert.AreEquivalent(Enumerable.Range(1, 11), enteredFloors);
            CollectionAssert.AreEquivalent(new[] { 0, 5, 10 }, preparedFloors, "The stock run must visit every preparation/boss reward chest.");
            Assert.That(game.Model.Floor, Is.EqualTo(11));
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Combat));
            Assert.That(game.Model.FloorsCleared, Is.EqualTo(10));
            Assert.That(game.Model.BossesDefeated, Is.EqualTo(2));
            Assert.That(game.Model.Inventory.MaxHealth, Is.EqualTo(130));
            Assert.That(game.Model.Inventory.AttackMultiplier, Is.EqualTo(1.15f).Within(.0001f));
            Assert.That(capturedCombat && capturedChest && capturedBoss, Is.True, "Every intended visual checkpoint must be reached.");
            Debug.Log("[scene run] stock run reached 11F; cease movement, firing and item use for natural AI death.");

            for (int tick = 0; tick < 60 * 120 && game.Model.Phase != ArenaPhase.Results; tick++)
            {
                game.AdvanceSimulation(Tick, default);
                if (tick % 120 == 0) yield return null;
            }
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Results));
            Assert.That(game.Model.Inventory.Health, Is.Zero);
            Assert.That(game.Model.Result.Floor, Is.EqualTo(11));
            Assert.That(game.Model.Result.FloorsCleared, Is.EqualTo(10));
            Debug.Log("[scene run] natural death " + Checkpoint());
            yield return CaptureRenderedFrame("result");
            ArenaResult result = game.Model.Result;
            game.AdvanceSimulation(.1f, new ArenaInput { FireA = true, UseB = true, Move = Vector2.up });
            Assert.That(game.Model.Result, Is.SameAs(result));
            Assert.That(game.Model.Elapsed, Is.EqualTo(result.Elapsed));
            neutralInput.KeyboardState = new KeyboardState(Key.R);
            double retryDeadline = Time.realtimeSinceStartupAsDouble + 5;
            while (game.Model.Phase == ArenaPhase.Results && Time.realtimeSinceStartupAsDouble < retryDeadline)
                yield return null;
            neutralInput.KeyboardState = new KeyboardState(Array.Empty<Key>());
            yield return null;
            yield return null;
            Assert.That(game.Model.Phase, Is.EqualTo(ArenaPhase.Preparing));
            Assert.That(game.Model.Floor, Is.Zero);
            Assert.That(game.Model.Result, Is.Null);
            Assert.That(game.Model.Inventory.Health, Is.EqualTo(100));
            Assert.That(game.Model.Inventory.MaxHealth, Is.EqualTo(100));
            Assert.That(game.Model.Inventory.AttackMultiplier, Is.EqualTo(1f));
            Assert.That(game.Model.Inventory.Backpack.All(item => item == null), Is.True);
            Assert.That(game.Model.Inventory.Equipment[0].Kind, Is.EqualTo(ItemKind.Blade));
            Assert.That(game.Model.Inventory.Equipment.Skip(1).All(item => item == null), Is.True);
            Assert.That(game.Model.EnemiesDefeated + game.Model.FloorsCleared + game.Model.BossesDefeated, Is.Zero);
            AssertPresentationMatches();
            // The runner still fails unexpected errors/exceptions. Informational
            // checkpoint logs above are intentional evidence, not failures.
            Debug.Log("[scene run] actual scene completed opening -> stock 1F..11F -> natural death -> clean retry.");
        }

        private void SelectStockSupplies()
        {
            foreach (var choice in new[]
            {
                ("Power Shooter", EquipmentSlot.WeaponA), ("Quick Shooter", EquipmentSlot.WeaponB),
                ("Shield", EquipmentSlot.ItemA), ("Medkit", EquipmentSlot.ItemB)
            })
            {
                int bag = TakeChestItem(item => item.Name == choice.Item1);
                bool equipped = false;
                game.InventoryAction(inventory => equipped = inventory.Equip(bag, choice.Item2));
                Assert.That(equipped, Is.True);
                if (game.Model.Inventory.Backpack[bag] != null)
                {
                    bool discarded = false;
                    game.InventoryAction(inventory => discarded = inventory.Discard(bag));
                    Assert.That(discarded, Is.True);
                }
            }
            foreach (ItemKind kind in new[] { ItemKind.HealthUpgrade, ItemKind.AttackUpgrade })
            {
                int bag = TakeChestItem(item => item.Kind == kind);
                bool used = false;
                game.InventoryAction(inventory => used = inventory.UseUpgrade(bag));
                Assert.That(used, Is.True);
            }
        }

        private static Vector2 MoveToChestAvoidingPortal(Vector2 position)
        {
            // A direct segment from above the portal to the chest can enter its trigger.
            // Move outward horizontally, then below the portal, before heading to supplies.
            // This changes only ordinary Move input; the portal and actor position stay live.
            const float clearance = ArenaSimulation.PortalRadius + .75f;
            Vector2 portalOffset = position - ArenaSimulation.PortalPosition;
            if (portalOffset.y > -clearance)
            {
                if (Mathf.Abs(portalOffset.x) < clearance)
                    return portalOffset.x < 0f ? Vector2.left : Vector2.right;
                return Vector2.down;
            }
            return (ArenaSimulation.ChestPosition - position).normalized;
        }

        private int TakeChestItem(Predicate<ArenaItem> match)
        {
            ArenaInventory inventory = game.Model.Inventory;
            int chestIndex = inventory.Chest.FindIndex(match);
            int bag = Array.FindIndex(inventory.Backpack, item => item == null);
            Assert.That(chestIndex, Is.GreaterThanOrEqualTo(0));
            Assert.That(bag, Is.GreaterThanOrEqualTo(0));
            ArenaItem expected = inventory.Chest[chestIndex];
            game.SelectedBackpack = bag;
            game.TakeChest(chestIndex);
            Assert.That(inventory.Backpack[bag], Is.SameAs(expected));
            return bag;
        }

        private IEnumerator CaptureRenderedFrame(string name)
        {
            // Request the normal rendered game view, including OnGUI. Never use
            // WaitForEndOfFrame, which Unity's batch test runner does not invoke.
            yield return null;
            yield return null;
            AssertPresentationMatches();
            if (Application.isBatchMode)
            {
                Debug.Log("[scene visual] SKIPPED in batch mode: " + name + "; native/interactive capture is required for screenshot evidence.");
                yield break;
            }
            string path = Path.Combine(visualDirectory, name + ".png");
            if (File.Exists(path)) File.Delete(path);
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
            {
                Debug.LogWarning("[scene visual] UNAVAILABLE: " + name + "; null graphics device. No visual evidence was produced.");
                yield break;
            }
            ScreenCapture.CaptureScreenshot(path);
            double deadline = Time.realtimeSinceStartupAsDouble + 3;
            while (!IsCompletePng(path) && Time.realtimeSinceStartupAsDouble < deadline) yield return null;
            if (IsCompletePng(path))
                Debug.Log("[scene visual] captured " + path + "; " + Checkpoint() + "; inspect image for visual quality.");
            else
                Debug.LogWarning("[scene visual] UNAVAILABLE: " + path + "; no completed PNG within three seconds. Scene assertions are not screenshot evidence.");
        }

        private static bool IsCompletePng(string path)
        {
            if (!File.Exists(path)) return false;
            byte[] bytes;
            try { bytes = File.ReadAllBytes(path); }
            catch (IOException) { return false; }
            return bytes.Length > 32 && bytes[0] == 137 && bytes[1] == 80 && bytes[2] == 78 && bytes[3] == 71 &&
                bytes[bytes.Length - 8] == 73 && bytes[bytes.Length - 7] == 69 && bytes[bytes.Length - 6] == 78 && bytes[bytes.Length - 5] == 68;
        }

        private void AssertPresentationMatches()
        {
            Assert.That(game.GetComponent<ArenaHud>().isActiveAndEnabled, Is.True);
            Assert.That(game.GetComponent<ArenaWorldView>().isActiveAndEnabled, Is.True);
            Assert.That(game.GameCamera.isActiveAndEnabled, Is.True);
            Transform stage = game.transform.Find("Arena presentation");
            Assert.That(stage, Is.Not.Null);
            bool running = game.Model.Phase != ArenaPhase.Opening;
            Assert.That(stage.gameObject.activeSelf, Is.EqualTo(running));
            Assert.That(stage.Find("Player").gameObject.activeInHierarchy, Is.EqualTo(running));
            int visibleEnemies = stage.Cast<Transform>().Count(child => child.name.StartsWith("enemy-", StringComparison.Ordinal) && child.gameObject.activeInHierarchy);
            Assert.That(visibleEnemies, Is.EqualTo(running ? game.Model.Enemies.Count : 0));
            Assert.That(stage.Find("Supply chest").gameObject.activeInHierarchy, Is.EqualTo(running && game.Model.ChestAvailable));
            Assert.That(stage.Find("Next floor portal").gameObject.activeInHierarchy, Is.EqualTo(running && game.Model.PortalAvailable));
        }

        private string Checkpoint() => game.Model.Floor + "F " + game.Model.Phase + "; t=" + game.Model.Elapsed.ToString("F2") +
            "; HP=" + game.Model.Inventory.Health + "/" + game.Model.Inventory.MaxHealth + "; enemies=" + game.Model.Enemies.Count +
            "; kills=" + game.Model.EnemiesDefeated + "; cleared=" + game.Model.FloorsCleared + "; bosses=" + game.Model.BossesDefeated;
    }
}
