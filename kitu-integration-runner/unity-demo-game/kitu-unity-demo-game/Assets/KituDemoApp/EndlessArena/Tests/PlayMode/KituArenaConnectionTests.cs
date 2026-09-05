using System;
using System.Collections;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using UnityEngine.TestTools.Utils;

namespace UnityOnlyArena.Tests
{
    public sealed class KituArenaConnectionTests
    {
        private GameObject root;
        [UnityTearDown]
        public IEnumerator Cleanup() { if (root != null) UnityEngine.Object.Destroy(root); yield return null; }

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
            client.Command("close");
            yield return Until(() => client.State.overlay == "none" && client.State.elapsed > chestTime, "close resumes game");
            yield return null;
        }

        private static IEnumerator Until(Func<bool> condition, string operation)
        {
            float deadline = Time.realtimeSinceStartup + 12f;
            while (!condition() && Time.realtimeSinceStartup < deadline) yield return null;
            Assert.That(condition(), Is.True, operation);
        }
    }
}
