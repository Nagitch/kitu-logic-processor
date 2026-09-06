using System;
using System.Collections;
using System.Text;
using System.Threading.Tasks;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Networking;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaNativeConnectionTests
    {
        private KituArenaClient client;
        private float previousTimeScale;

        [UnitySetUp]
        public IEnumerator Setup()
        {
            if (Application.platform != RuntimePlatform.OSXEditor && Application.platform != RuntimePlatform.OSXPlayer)
                Assert.Ignore("Native Arena is currently built for macOS.");
            previousTimeScale = Time.timeScale;
#if UNITY_EDITOR
            yield return UnityEditor.SceneManagement.EditorSceneManager.LoadSceneAsyncInPlayMode(
                "Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity", new LoadSceneParameters(LoadSceneMode.Single));
#else
            yield return SceneManager.LoadSceneAsync("KituEndlessArena", LoadSceneMode.Single);
#endif
            client = UnityEngine.Object.FindFirstObjectByType<KituArenaClient>();
            client.NativeConnection?.Dispose();
            client.ConnectOnStart = false;
            client.Backend = ArenaBackend.Embedded;
            client.NativeBridgeEnabled = false;
            client.DeviceInput = false;
            client.PauseOnFocusLoss = false;
            client.Connect();
            yield return Until(() => client.Connected, "native handshake");
        }

        [UnityTearDown]
        public IEnumerator Cleanup()
        {
            Time.timeScale = previousTimeScale;
            if (client != null)
            {
                var owner = client.NativeConnection;
                client.gameObject.SetActive(false);
                if (owner != null) Assert.That(owner.IsDisposed, Is.True, "OnDisable must release native ownership synchronously");
                UnityEngine.Object.Destroy(client.gameObject);
            }
            yield return null;
        }

        [UnityTest, Category("ArenaNative")]
        public IEnumerator NativeClockContinuesDuringPauseAndDetachAndReconnectKeepsTheRun()
        {
            var owner = client.NativeConnection;
            string session = client.SessionId;
            Assert.That(Application.runInBackground, Is.True);
            Assert.That(client.Command("unsupported-test-command"), Is.False, "native driver refusal is nonfatal");
            Assert.That(client.Connected, Is.True);
            client.Command("start");
            yield return Until(() => client.State.phase == 1, "native start");
            client.Frame(Vector2.up, Vector2.up);
            yield return Until(() => client.State.playerPosition.y > -6.8f, "native movement");
            client.Command("pause");
            yield return Until(() => client.State.overlay == "pause", "native pause");
            float elapsed = client.State.elapsed;
            long tick = client.State.tick;
            Time.timeScale = 0;
            yield return Until(() => client.State.tick > tick + 8, "unscaled paused management clock");
            Assert.That(client.State.elapsed, Is.EqualTo(elapsed));
            client.Command("resume");
            yield return Until(() => client.State.overlay == "none", "native resume");
            client.Frame(Vector2.up, Vector2.up);
            client.Disconnect();
            Assert.That(client.Connected, Is.False);
            yield return Until(() => client.State.overlay == "pause", "detach pauses and clears continuous controls");
            elapsed = client.State.elapsed;
            tick = client.State.tick;
            Vector2 position = client.State.playerPosition;
            yield return Until(() => client.State.tick > tick + 8, "detached management still ticks");
            Assert.That(client.State.elapsed, Is.EqualTo(elapsed));
            Assert.That(client.State.playerPosition, Is.EqualTo(position));
            client.Connect();
            yield return Until(() => client.Connected, "native reattachment");
            Assert.That(client.NativeConnection, Is.SameAs(owner));
            Assert.That(client.SessionId, Is.EqualTo(session));
            Assert.That(client.State.overlay, Is.EqualTo("pause"));
            client.Command("resume");
            yield return Until(() => client.State.elapsed > elapsed, "explicit resume after native reconnect");
            Assert.That(client.State.playerPosition, Is.EqualTo(position), "detachment must release the earlier held movement");
        }

        [UnityTest, Category("ArenaNative")]
        public IEnumerator DisableAndReenableReleasesOwnershipAndWrongThreadCallsNeverEnterFfi()
        {
            client.NativeAutomaticTicks = false;
            yield return null;
            var owner = client.NativeConnection;
            string initial = owner.InspectStateJson();
            var wrongThread = Task.Run(() => owner.InspectStateJson());
            yield return Until(() => wrongThread.IsCompleted, "wrong-thread guard");
            Assert.That(wrongThread.IsFaulted, Is.True);
            Assert.That(wrongThread.Exception.InnerException, Is.TypeOf<InvalidOperationException>());
            Assert.That(owner.InspectStateJson(), Is.EqualTo(initial));
            for (int cycle = 0; cycle < 3; cycle++)
            {
                string session = client.SessionId;
                int live = ArenaNativeConnection.LiveHandleCount;
                client.gameObject.SetActive(false);
                Assert.That(owner.IsDisposed, Is.True);
                Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(live - 1));
                client.gameObject.SetActive(true);
                client.Connect();
                yield return Until(() => client.Connected, "fresh native owner after enable");
                Assert.That(client.NativeConnection, Is.Not.SameAs(owner));
                Assert.That(client.SessionId, Is.Not.EqualTo(session));
                Assert.That(client.State.tick, Is.EqualTo(-1));
                Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(live));
                owner = client.NativeConnection;
            }
        }

        [UnityTest, Category("ArenaNative")]
        public IEnumerator EmbeddedBridgeShellAndReplayControlTheSameProjectedRun()
        {
            client.NativeConnection.Dispose();
            client.NativeBridgeEnabled = true;
            client.NativeBridgeAddress = "127.0.0.1:0";
            client.Connect();
            yield return Until(() => client.Connected && client.NativeConnection.BridgeEndpoint != null, "native bridge");
            string api = client.NativeConnection.BridgeEndpoint.TrimEnd('/');
            JObject catalog = null;
            yield return Request(api, "/shell/catalog", null, value => catalog = value);
            Assert.That((string)catalog["sessionId"], Is.EqualTo(client.SessionId));
            JObject reply = null;
            yield return Request(api, "/shell/line", new JObject {
                ["version"] = 1, ["sessionId"] = client.SessionId, ["clientId"] = "native-test-" + Guid.NewGuid().ToString("N"),
                ["id"] = 1, ["line"] = "app action run arena.start",
            }, value => reply = value);
            Assert.That((bool)reply["ok"], Is.True, reply.ToString());
            yield return Until(() => client.State.phase == 1, "embedded Shell starts the Unity run");
            client.Command("pause");
            yield return Until(() => client.State.overlay == "pause", "recorded pause");
            JObject saved = null;
            yield return Request(api, "/arena/recording/save", new JObject(), value => saved = value);
            string recording = (string)saved["id"];
            Assert.That(recording, Has.Length.EqualTo(64));
            yield return Request(api, "/arena/playback/load", new JObject { ["id"] = recording });
            yield return Until(() => client.ReplayActive && client.State.tick == -1, "native replay projection");
            Assert.That(client.Command("start"), Is.False);
            Assert.That(client.NativeConnection.ReadOnly, Is.True);
            yield return Request(api, "/arena/playback/seek", new JObject { ["tick"] = 0 });
            yield return Until(() => client.State.tick == 0, "native replay seek");
            JObject inspected = null;
            yield return Request(api, "/arena/playback", null, value => inspected = value);
            var expected = JsonUtility.FromJson<ArenaReferenceState>(inspected["state"].ToString(Formatting.None));
            Assert.That(JsonUtility.ToJson(client.State), Is.EqualTo(JsonUtility.ToJson(expected)));
            client.Disconnect();
            client.Connect();
            yield return Until(() => client.Connected && client.ReplayActive && client.State.tick == 0, "native replay reattachment");
            yield return Request(api, "/arena/playback/command", new JObject { ["action"] = "step" });
            yield return Until(() => client.State.tick == 1, "native replay step");
            yield return Request(api, "/arena/playback/command", new JObject { ["action"] = "live" });
            yield return Until(() => !client.ReplayActive && client.State.overlay == "pause", "native replay returns to paused live state");
            float elapsed = client.State.elapsed;
            client.Command("resume");
            yield return Until(() => client.State.elapsed > elapsed, "native live resume");
            Debug.Log("Embedded bridge verified: common Shell, record/save, replay/seek/step/reconnect, paused live return.");
        }

        private static IEnumerator Request(string api, string path, JObject body, Action<JObject> receive = null)
        {
            using (var request = new UnityWebRequest(api + path, body == null ? "GET" : "POST"))
            {
                request.downloadHandler = new DownloadHandlerBuffer();
                if (body != null)
                {
                    request.uploadHandler = new UploadHandlerRaw(Encoding.UTF8.GetBytes(body.ToString(Formatting.None)));
                    request.SetRequestHeader("Content-Type", "application/json");
                }
                request.timeout = 20;
                yield return request.SendWebRequest();
                Assert.That(request.result, Is.EqualTo(UnityWebRequest.Result.Success), request.downloadHandler.text);
                receive?.Invoke(JObject.Parse(request.downloadHandler.text));
            }
        }

        private static IEnumerator Until(Func<bool> condition, string description)
        {
            double deadline = Time.realtimeSinceStartupAsDouble + 15;
            while (!condition() && Time.realtimeSinceStartupAsDouble < deadline) yield return null;
            Assert.That(condition(), Is.True, description);
        }
    }
}
