using System;
using UnityEngine;
using UnityEngine.InputSystem;

namespace UnityOnlyArena
{
    public enum ArenaOverlay { None, Pause, Inventory, Chest, Settings }

    // Unity owns input, screen state and presentation. ArenaSimulation owns the live run.
    public sealed class ArenaGame : MonoBehaviour
    {
        private const float StepSeconds = 1f / 60f;
        private float accumulator;
        private bool queuedUseA, queuedUseB;
        private readonly bool[] requireRelease = { true, true, true, true };
        private ArenaOverlay settingsOrigin;
        private string lastSimulationMessage = "";
        private ArenaWorldView world;

        public ArenaSimulation Model { get; private set; }
        public ArenaSettings Settings { get; private set; }
        public ArenaSettings DraftSettings { get; private set; }
        public ArenaOverlay Overlay { get; private set; }
        public int SelectedBackpack { get; set; }
        public string Message { get; private set; } = "";
        public Camera GameCamera => world != null ? world.GameCamera : null;
        public bool IsSafe => Model != null && (Model.Phase == ArenaPhase.Preparing || Model.Phase == ArenaPhase.Cleared);
        public bool IsSimulationRunning => Model != null && Overlay == ArenaOverlay.None &&
            Model.Phase != ArenaPhase.Opening && Model.Phase != ArenaPhase.Results;

        private void Awake()
        {
            Model = new ArenaSimulation();
            Settings = ArenaSettings.Load();
            Settings.Apply(false);
            world = gameObject.AddComponent<ArenaWorldView>();
            world.Initialize(Model);
            gameObject.AddComponent<ArenaHud>().Initialize(this);
            Cursor.visible = true;
            Cursor.lockState = CursorLockMode.None;
        }

        public void StartRun()
        {
            if (Model.Phase != ArenaPhase.Opening && Model.Phase != ArenaPhase.Results) return;
            Model.StartRun();
            lastSimulationMessage = Model.LastMessage;
            Overlay = ArenaOverlay.None;
            Message = "Choose supplies at the chest, then enter the portal.";
            SelectedBackpack = 0;
            BlockGameplayButtons();
        }

        public void ReturnToMenu()
        {
            Model.ReturnToMenu();
            lastSimulationMessage = Model.LastMessage;
            Overlay = ArenaOverlay.None;
            Message = "";
            BlockGameplayButtons();
        }

        public void SetPaused(bool paused)
        {
            if (Model.Phase == ArenaPhase.Opening || Model.Phase == ArenaPhase.Results) return;
            Overlay = paused ? ArenaOverlay.Pause : ArenaOverlay.None;
            BlockGameplayButtons();
        }

        public void OpenInventory()
        {
            if (!IsSafe) { Message = "Equipment can be changed after clearing the floor."; return; }
            Overlay = ArenaOverlay.Inventory;
            BlockGameplayButtons();
        }

        public void OpenChest()
        {
            if (!IsSafe || !Model.ChestAvailable || Vector2.Distance(Model.PlayerPosition, ArenaSimulation.ChestPosition) > 2f)
            { Message = "Move within 2 units of the chest to open it (E)."; return; }
            Overlay = ArenaOverlay.Chest;
            BlockGameplayButtons();
        }

        public void CloseOverlay()
        {
            if (Overlay == ArenaOverlay.Settings) { CancelSettings(); return; }
            Overlay = ArenaOverlay.None;
            BlockGameplayButtons();
        }

        public void OpenSettings()
        {
            if (Model.Phase != ArenaPhase.Opening && Overlay != ArenaOverlay.Pause) return;
            settingsOrigin = Overlay;
            DraftSettings = Settings.Copy();
            Overlay = ArenaOverlay.Settings;
            BlockGameplayButtons();
        }

        public void ApplySettings()
        {
            if (Overlay != ArenaOverlay.Settings) return;
            Settings = DraftSettings.Copy();
            Settings.Apply();
            Overlay = settingsOrigin;
            BlockGameplayButtons();
        }

        public void CancelSettings()
        {
            if (Overlay != ArenaOverlay.Settings) return;
            DraftSettings = null;
            Overlay = settingsOrigin;
            BlockGameplayButtons();
        }

        public void ResetSettingsDraft()
        {
            if (Overlay == ArenaOverlay.Settings) DraftSettings = new ArenaSettings();
        }

        public void InventoryAction(Func<ArenaInventory, bool> action)
        {
            if (!IsSafe || (Overlay != ArenaOverlay.Inventory && Overlay != ArenaOverlay.Chest)) return;
            action(Model.Inventory);
            Message = Model.Inventory.LastMessage;
        }

        public void TakeChest(int index)
        {
            if (Overlay != ArenaOverlay.Chest) return;
            InventoryAction(i => i.TakeChest(index, SelectedBackpack));
        }

        public void Quit()
        {
#if UNITY_EDITOR
            Message = "Quit closes the standalone player. The Unity Editor remains open.";
#else
            Application.Quit();
#endif
        }

        private void Update()
        {
            var keyboard = Keyboard.current;
            if (keyboard != null)
            {
                if (keyboard.escapeKey.wasPressedThisFrame)
                {
                    if (Overlay != ArenaOverlay.None) CloseOverlay();
                    else SetPaused(true);
                }
                if (keyboard.tabKey.wasPressedThisFrame)
                {
                    if (Overlay == ArenaOverlay.Inventory || Overlay == ArenaOverlay.Chest) CloseOverlay();
                    else if (Overlay == ArenaOverlay.None) OpenInventory();
                }
                if (keyboard.eKey.wasPressedThisFrame && Overlay == ArenaOverlay.None) OpenChest();
                if (keyboard.rKey.wasPressedThisFrame && Model.Phase == ArenaPhase.Results) StartRun();
            }

            var mouse = Mouse.current;
            bool[] down = {
                mouse != null && mouse.leftButton.isPressed,
                mouse != null && mouse.rightButton.isPressed,
                keyboard != null && keyboard.zKey.isPressed,
                keyboard != null && keyboard.xKey.isPressed,
            };
            for (int i = 0; i < 4; i++) if (!down[i]) requireRelease[i] = false;
            if (!IsSimulationRunning) { accumulator = 0f; queuedUseA = queuedUseB = false; return; }

            var input = new ArenaInput();
            if (keyboard != null)
            {
                input.Move = new Vector2((keyboard.dKey.isPressed ? 1 : 0) - (keyboard.aKey.isPressed ? 1 : 0),
                    (keyboard.wKey.isPressed ? 1 : 0) - (keyboard.sKey.isPressed ? 1 : 0));
                input.UseA = !requireRelease[2] && keyboard.zKey.wasPressedThisFrame;
                input.UseB = !requireRelease[3] && keyboard.xKey.wasPressedThisFrame;
            }
            input.FireA = !requireRelease[0] && down[0];
            input.FireB = !requireRelease[1] && down[1];
            if (mouse != null && GameCamera != null)
            {
                Ray ray = GameCamera.ScreenPointToRay(mouse.position.ReadValue());
                if (new Plane(Vector3.up, Vector3.zero).Raycast(ray, out float distance))
                {
                    Vector3 point = ray.GetPoint(distance);
                    input.AimPoint = new Vector2(point.x, point.z);
                    input.HasAim = true;
                }
            }
            AdvanceSimulation(Time.unscaledDeltaTime, input);
        }

        // Public so controller/menu tests can exercise the same driver without a hardware keyboard.
        public void AdvanceSimulation(float elapsed, ArenaInput input)
        {
            if (!IsSimulationRunning) return;
            queuedUseA |= input.UseA;
            queuedUseB |= input.UseB;
            accumulator += Mathf.Clamp(elapsed, 0f, 0.1f);
            while (accumulator >= StepSeconds && IsSimulationRunning)
            {
                input.UseA = queuedUseA;
                input.UseB = queuedUseB;
                Model.Step(StepSeconds, input);
                if (Model.LastMessage != lastSimulationMessage)
                {
                    lastSimulationMessage = Model.LastMessage;
                    Message = lastSimulationMessage;
                }
                queuedUseA = queuedUseB = false;
                accumulator -= StepSeconds;
            }
        }

        private void BlockGameplayButtons()
        {
            for (int i = 0; i < requireRelease.Length; i++) requireRelease[i] = true;
            accumulator = 0f;
            queuedUseA = queuedUseB = false;
        }

        private void LateUpdate() => world.Sync(Model);

        private void OnApplicationFocus(bool focused)
        {
            if (!focused && IsSimulationRunning) SetPaused(true);
        }
    }
}
