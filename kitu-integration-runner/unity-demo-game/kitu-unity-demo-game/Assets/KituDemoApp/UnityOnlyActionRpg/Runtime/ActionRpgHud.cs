using UnityEngine;

namespace UnityOnlyActionRpg
{
    public sealed class ActionRpgHud : MonoBehaviour
    {
        private ActionRpgGameController game;
        private GUIStyle titleStyle;
        private GUIStyle labelStyle;
        private GUIStyle centeredStyle;

        private void Start()
        {
            game = FindFirstObjectByType<ActionRpgGameController>();
        }

        private void OnGUI()
        {
            if (game == null || game.Player == null)
            {
                return;
            }

            EnsureStyles();

            ActionRpgPlayerController player = game.Player;
            GUI.Box(new Rect(16f, 16f, 520f, 174f), string.Empty);
            GUI.Label(new Rect(30f, 24f, 480f, 28f), "UNITY-ONLY ACTION RPG DEMO", titleStyle);

            GUI.Label(new Rect(30f, 56f, 480f, 22f), $"HP  {player.CurrentHealth}/{player.MaxHealth}", labelStyle);
            DrawBar(new Rect(160f, 59f, 340f, 16f), (float)player.CurrentHealth / player.MaxHealth, new Color(0.8f, 0.15f, 0.15f));

            GUI.Label(new Rect(30f, 82f, 480f, 22f), $"Level {game.Level}  XP {game.Experience}/{game.ExperienceToNextLevel}  Attack {player.AttackDamage}", labelStyle);
            GUI.Label(new Rect(30f, 108f, 480f, 22f), game.ObjectiveText, labelStyle);
            GUI.Label(new Rect(30f, 134f, 480f, 22f), $"Potions: {game.Potions} (E / gamepad X to heal 40)", labelStyle);
            GUI.Label(new Rect(30f, 158f, 480f, 22f), "Move: WASD / arrows / left stick   Attack: Space / J / click / gamepad A", labelStyle);

            if (game.IsComplete)
            {
                DrawResult("OBJECTIVE COMPLETE\nPress R to restart", new Color(0.15f, 0.65f, 0.25f, 0.95f));
            }
            else if (!game.IsRunning && player.CurrentHealth <= 0)
            {
                DrawResult("YOU WERE DEFEATED\nPress R to restart", new Color(0.7f, 0.12f, 0.12f, 0.95f));
            }
        }

        private static void DrawBar(Rect rect, float ratio, Color color)
        {
            GUI.Box(rect, string.Empty);
            Color previous = GUI.color;
            GUI.color = color;
            GUI.DrawTexture(new Rect(rect.x + 2f, rect.y + 2f, (rect.width - 4f) * Mathf.Clamp01(ratio), rect.height - 4f), Texture2D.whiteTexture);
            GUI.color = previous;
        }

        private void DrawResult(string message, Color color)
        {
            Rect panel = new(Screen.width * 0.5f - 230f, Screen.height * 0.5f - 80f, 460f, 160f);
            Color previous = GUI.color;
            GUI.color = color;
            GUI.Box(panel, string.Empty);
            GUI.color = previous;
            GUI.Label(panel, message, centeredStyle);
        }

        private void EnsureStyles()
        {
            if (titleStyle != null)
            {
                return;
            }

            titleStyle = new GUIStyle(GUI.skin.label)
            {
                fontSize = 18,
                fontStyle = FontStyle.Bold,
                normal = { textColor = Color.white },
            };
            labelStyle = new GUIStyle(GUI.skin.label)
            {
                fontSize = 14,
                normal = { textColor = Color.white },
            };
            centeredStyle = new GUIStyle(titleStyle)
            {
                alignment = TextAnchor.MiddleCenter,
                fontSize = 28,
            };
        }
    }
}
