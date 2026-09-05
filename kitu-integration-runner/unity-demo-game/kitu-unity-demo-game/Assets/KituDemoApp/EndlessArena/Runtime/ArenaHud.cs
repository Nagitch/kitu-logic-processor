using System;
using UnityEngine;

namespace UnityOnlyArena
{
    public sealed class ArenaHud : MonoBehaviour
    {
        private ArenaGame game;
        private GUIStyle title, heading, text, small, button;
        private Vector2 chestScroll;
        private readonly Color accent = new Color(.35f, .9f, .76f);
        private readonly string[] slotNames = { "WEAPON A / LMB", "WEAPON B / RMB", "ITEM A / Z", "ITEM B / X" };

        public void Initialize(ArenaGame owner) => game = owner;

        private void OnGUI()
        {
            if (game == null || game.Model == null) return;
            Styles();
            ClearOutsideCamera();
            DrawEnemyLabels();
            Matrix4x4 previous = GUI.matrix;
            float scale = Mathf.Min(Screen.width / 1280f, Screen.height / 800f);
            GUI.matrix = Matrix4x4.TRS(new Vector3((Screen.width - 1280 * scale) * .5f, (Screen.height - 800 * scale) * .5f), Quaternion.identity, Vector3.one * scale);
            var model = game.Model;
            if (model.Phase != ArenaPhase.Opening) DrawHud();
            if (game.Overlay == ArenaOverlay.Settings) DrawSettings();
            else if (model.Phase == ArenaPhase.Opening) DrawOpening();
            else if (model.Phase == ArenaPhase.Results) DrawResults();
            else if (game.Overlay == ArenaOverlay.Pause) DrawPause();
            else if (game.Overlay == ArenaOverlay.Inventory || game.Overlay == ArenaOverlay.Chest) DrawInventory();
            else if (model.Phase == ArenaPhase.Transition)
            {
                Panel(new Rect(440, 340, 400, 100));
                Label(475, 375, 340, "Entering the next floor...", heading);
            }
            GUI.matrix = previous;
        }

        private void ClearOutsideCamera()
        {
            // The world camera clears only its viewport. Clear the HUD bands as
            // well so changing text cannot accumulate outside that viewport.
            if (game.GameCamera == null) return;
            Rect viewport = game.GameCamera.rect;
            Color previous = GUI.color;
            GUI.color = Color.black;
            GUI.DrawTexture(new Rect(0, 0, Screen.width, Screen.height * (1f - viewport.yMax)), Texture2D.whiteTexture);
            GUI.DrawTexture(new Rect(0, Screen.height * (1f - viewport.y), Screen.width, Screen.height * viewport.y), Texture2D.whiteTexture);
            GUI.color = previous;
        }

        private void DrawOpening()
        {
            Panel(new Rect(360, 140, 560, 530));
            Label(410, 185, 490, "ENDLESS ARENA", title);
            Label(410, 235, 490, "UNITY-ONLY  /  SYSTEMS PROTOTYPE", small);
            if (Button(410, 285, 460, 52, "Start game")) game.StartRun();
            if (Button(410, 353, 460, 48, "Settings")) game.OpenSettings();
            if (Button(410, 417, 460, 48, "Quit")) game.Quit();
            Label(410, 495, 460, "WASD Move   Mouse Aim   LMB / RMB Weapons\nZ / X Items   E Chest   Tab Inventory   Esc Pause", text, 60);
            Label(410, 581, 460, game.Message, small, 65);
        }

        private void DrawSettings()
        {
            Panel(new Rect(350, 135, 580, 535));
            Label(390, 170, 510, "SETTINGS", title);
            Label(390, 237, 490, "Master volume  " + Mathf.RoundToInt(game.DraftSettings.Volume * 100) + "%", heading);
            game.DraftSettings.Volume = GUI.HorizontalSlider(new Rect(390, 290, 500, 28), game.DraftSettings.Volume, 0, 1);
            if (Button(390, 338, 500, 46, "Display: " + (game.DraftSettings.Fullscreen ? "Borderless fullscreen" : "Windowed")))
                game.DraftSettings.Fullscreen = !game.DraftSettings.Fullscreen;
            Label(390, 402, 500, "Changes are saved only when applied.\nDisplay changes apply to the standalone player.", small, 50);
            if (Button(390, 473, 500, 42, "Restore defaults")) game.ResetSettingsDraft();
            if (Button(390, 539, 240, 48, "Apply and return")) game.ApplySettings();
            if (Button(650, 539, 240, 48, "Cancel")) game.CancelSettings();
        }

        private void DrawPause()
        {
            Panel(new Rect(375, 210, 530, 385));
            Label(425, 250, 450, "PAUSED", title);
            if (Button(425, 319, 430, 50, "Resume")) game.SetPaused(false);
            if (Button(425, 385, 430, 48, "Settings")) game.OpenSettings();
            if (Button(425, 451, 430, 48, "Return to menu (end this run)")) game.ReturnToMenu();
            Label(425, 526, 440, "Combat, shield recovery and the run clock are stopped.", small, 50);
        }

        private void DrawResults()
        {
            var result = game.Model.Result;
            if (result == null) return;
            Panel(new Rect(325, 130, 630, 540));
            Label(370, 165, 540, "GAME OVER", title);
            Label(370, 221, 540, $"Reached floor {result.Floor}    /    Cleared {result.FloorsCleared}", heading);
            Label(370, 267, 540, $"Defeated {result.EnemiesDefeated} enemies, including {result.BossesDefeated} bosses\nRun time {Clock(result.Elapsed)}\nMaximum HP {result.MaxHealth}    Damage x{result.AttackMultiplier:0.00}", text, 85);
            for (int i = 0; i < 4; i++) Label(370, 365 + i * 31, 550, slotNames[i] + ":  " + result.EquipmentNames[i], text);
            if (Button(370, 528, 260, 50, "Try again (R)")) game.StartRun();
            if (Button(650, 528, 260, 50, "Return to menu")) game.ReturnToMenu();
        }

        private void DrawHud()
        {
            var model = game.Model;
            var inv = model.Inventory;
            Panel(new Rect(16, 12, 1248, 103));
            Label(34, 23, 525, "ENDLESS ARENA  /  UNITY-ONLY", heading);
            Label(720, 25, 520, $"FLOOR {model.Floor}{(model.IsBossFloor ? "  BOSS" : "")}    ENEMIES {model.Enemies.Count}    {Clock(model.Elapsed)}", heading);
            Bar(new Rect(34, 67, 265, 25), (float)inv.Health / inv.MaxHealth, new Color(.8f, .24f, .3f));
            Label(45, 67, 250, $"HP {inv.Health} / {inv.MaxHealth}", text);
            string objective = model.Phase == ArenaPhase.Combat ? "Defeat every enemy" : model.Phase == ArenaPhase.Preparing ? "Prepare at the chest, then enter the portal" :
                model.Phase == ArenaPhase.Cleared ? "Floor cleared! Enter the portal for floor " + (model.Floor + 1) : model.Phase.ToString();
            Label(325, 68, 900, objective, text);
            for (int i = 0; i < 4; i++)
            {
                float x = 18 + i * 312;
                Panel(new Rect(x, 659, 308, 93));
                Label(x + 12, 667, 280, slotNames[i], small);
                ArenaItem item = inv.Equipment[i];
                Label(x + 12, 691, 282, item == null ? "Empty" : item.Name, heading);
                string status = item == null ? "" : i < 2 ? (model.WeaponCooldowns[i] > 0 ? $"Cooldown {model.WeaponCooldowns[i]:0.0}s" : "Ready") :
                    item.Kind == ItemKind.Shield ? $"AUTO {item.Shield}/{inv.MaxHealth}  " + (inv.ShieldDelayRemaining > 0 ? $"wait {inv.ShieldDelayRemaining:0.0}s" : item.Shield < inv.MaxHealth ? "recharging" : "full") : "Single use";
                Label(x + 12, 724, 283, status, small);
            }
            int carried = 0;
            foreach (var item in inv.Backpack) if (item != null) carried++;
            Label(25, 765, 710, $"Backpack {carried}/3   |   E Chest   Tab Inventory   Esc Pause", small);
            Label(720, 760, 540, game.Message, small, 38);
            if (model.PortalAvailable) WorldLabel(ArenaSimulation.PortalPosition, $"PORTAL -> {model.Floor + 1}F", accent);
            if (model.ChestAvailable) WorldLabel(ArenaSimulation.ChestPosition + Vector2.down, "SUPPLIES [E]", new Color(1f, .8f, .3f));
        }

        private void DrawInventory()
        {
            var inv = game.Model.Inventory;
            bool showChest = game.Overlay == ArenaOverlay.Chest;
            Panel(new Rect(25, 125, 1230, 520));
            Label(45, 140, 580, showChest ? "SUPPLIES / Choose what to carry" : "INVENTORY / Safe floor", heading);
            if (Button(1123, 137, 110, 32, "Close")) game.CloseOverlay();
            Label(45, 179, 1170, "Select a backpack slot, then take/swap supplies or equip its item. All gameplay is paused.", small);
            if (showChest)
            {
                Label(45, 214, 480, "CHEST  /  Leftovers disappear on the next floor", small);
                chestScroll = GUI.BeginScrollView(new Rect(42, 247, 480, 351), chestScroll, new Rect(0, 0, 451, Mathf.Max(350, inv.Chest.Count * 63)));
                for (int i = 0; i < inv.Chest.Count; i++)
                {
                    var item = inv.Chest[i];
                    Label(8, i * 63 + 2, 327, item.Name, text);
                    Label(8, i * 63 + 27, 327, Describe(item, inv), small);
                    if (Button(342, i * 63 + 8, 97, 36, inv.Backpack[game.SelectedBackpack] == null ? "Take" : "Swap"))
                    { game.TakeChest(i); break; }
                }
                GUI.EndScrollView();
            }
            else
                Label(55, 250, 430, "Only equipped items work during combat.\n\nZ / X use medkits and grenades once.\nShields absorb damage automatically.\n\nAn upgrade is consumed from the backpack.\n\nOpen a nearby supply chest with E to compare and exchange items.", text, 300);
            Label(550, 214, 275, "BACKPACK / 3 slots", small);
            for (int i = 0; i < 3; i++)
            {
                Color old = GUI.backgroundColor;
                if (i == game.SelectedBackpack) GUI.backgroundColor = accent;
                if (Button(544, 250 + i * 89, 278, 80, $"{i + 1}  " + (inv.Backpack[i]?.Name ?? "Empty") + "\n" + Describe(inv.Backpack[i], inv))) game.SelectedBackpack = i;
                GUI.backgroundColor = old;
            }
            if (Button(545, 526, 276, 34, "Use selected upgrade")) game.InventoryAction(x => x.UseUpgrade(game.SelectedBackpack));
            if (Button(545, 570, 276, 34, "Discard selected (permanent)")) game.InventoryAction(x => x.Discard(game.SelectedBackpack));
            Label(846, 214, 365, "EQUIPMENT / 4 slots", small);
            for (int i = 0; i < 4; i++)
            {
                int index = i;
                Label(847, 249 + i * 85, 362, slotNames[i] + ": " + (inv.Equipment[i]?.Name ?? "Empty"), small);
                Label(847, 270 + i * 85, 362, Describe(inv.Equipment[i], inv), small);
                if (Button(847, 296 + i * 85, 173, 29, "Equip / swap")) game.InventoryAction(x => x.Equip(game.SelectedBackpack, (EquipmentSlot)index));
                if (Button(1031, 296 + i * 85, 175, 29, "Unequip")) game.InventoryAction(x => x.Unequip((EquipmentSlot)index));
            }
            Label(50, 611, 1150, $"Max HP {inv.MaxHealth}   Damage x{inv.AttackMultiplier:0.00}   Move {ArenaSimulation.PlayerSpeed}/s   |   {game.Message}", small);
        }

        private static string Describe(ArenaItem item, ArenaInventory inv)
        {
            if (item == null) return "";
            if (item.IsWeapon) return $"Damage {item.Damage} / {item.Interval:0.##}s";
            if (item.Kind == ItemKind.Shield) return $"Shield {item.Shield}/{inv.MaxHealth} / auto";
            if (item.Kind == ItemKind.Medkit) return "Full HP / consumed on use";
            if (item.Kind == ItemKind.Grenade) return "100 area damage / consumed on throw";
            return item.Kind == ItemKind.HealthUpgrade ? "+10 maximum and current HP" : "+5% damage for this run";
        }

        private void DrawEnemyLabels()
        {
            if (game.Model.Phase == ArenaPhase.Opening || game.GameCamera == null) return;
            foreach (var enemy in game.Model.Enemies)
            {
                Vector3 screen = game.GameCamera.WorldToScreenPoint(ArenaWorldView.Point(enemy.Position + Vector2.up * (enemy.Radius + .5f), 0));
                Rect rect = new Rect(screen.x - 37, Screen.height - screen.y - 12, 74, 17);
                Bar(rect, (float)enemy.Health / enemy.MaxHealth, new Color(.8f, .18f, .23f));
                GUI.Label(rect, $"{enemy.Health}/{enemy.MaxHealth}", small);
            }
        }

        private void WorldLabel(Vector2 point, string value, Color color)
        {
            if (game.GameCamera == null) return;
            Vector3 screen = game.GameCamera.WorldToScreenPoint(ArenaWorldView.Point(point, 0));
            Matrix4x4 matrix = GUI.matrix;
            GUI.matrix = Matrix4x4.identity;
            Color previous = GUI.color;
            GUI.color = color;
            GUI.Label(new Rect(screen.x - 95, Screen.height - screen.y + 15, 205, 25), value, small);
            GUI.color = previous;
            GUI.matrix = matrix;
        }

        private void Styles()
        {
            if (text != null) return;
            text = new GUIStyle(GUI.skin.label) { fontSize = 16, wordWrap = true, normal = { textColor = Color.white } };
            small = new GUIStyle(text) { fontSize = 13, normal = { textColor = new Color(.79f, .84f, .9f) } };
            heading = new GUIStyle(text) { fontSize = 19, fontStyle = FontStyle.Bold };
            title = new GUIStyle(heading) { fontSize = 34, normal = { textColor = accent } };
            button = new GUIStyle(GUI.skin.button) { fontSize = 15, wordWrap = true };
        }

        private static void Panel(Rect rect)
        {
            Color old = GUI.color;
            GUI.color = new Color(.055f, .08f, .115f, 1f);
            GUI.DrawTexture(rect, Texture2D.whiteTexture);
            GUI.color = old;
        }

        private static void Bar(Rect rect, float fraction, Color color)
        {
            Panel(rect);
            Color old = GUI.color;
            GUI.color = color;
            GUI.DrawTexture(new Rect(rect.x, rect.y, rect.width * Mathf.Clamp01(fraction), rect.height), Texture2D.whiteTexture);
            GUI.color = old;
        }

        private static void Label(float x, float y, float width, string value, GUIStyle style, float height = 30) =>
            GUI.Label(new Rect(x, y, width, Mathf.Max(height, style.lineHeight + 8)), value, style);
        private bool Button(float x, float y, float width, float height, string value) => GUI.Button(new Rect(x, y, width, height), value, button);
        private static string Clock(float seconds) => TimeSpan.FromSeconds(seconds).ToString(@"hh\:mm\:ss");
    }
}
