using System;
using System.Collections;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.Networking;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaInspectionTests
    {
        private KituArenaClient client;
        private string storage;
        private readonly JArray evidence = new JArray();

        [UnityTearDown]
        public IEnumerator Cleanup()
        {
            if (client != null)
            {
                client.gameObject.SetActive(false);
                UnityEngine.Object.Destroy(client.gameObject);
            }
            yield return null;
            yield return null;
            if (storage != null && Directory.Exists(storage)) Directory.Delete(storage, true);
            storage = null;
        }

        [UnityTest, Category("ArenaNative")]
        public IEnumerator EmbeddedInspectionReadsMatchUnityWithoutAdvancingTheManualOwner()
        {
            if (Application.platform != RuntimePlatform.OSXEditor)
                Assert.Ignore("This fixture requires the macOS native Arena plugin.");
            yield return LoadClient(ArenaBackend.Embedded);
            var owner = client.NativeConnection;
            string api = owner.BridgeEndpoint;
            Assert.That(api, Is.Not.Null);
            Assert.That(client.State.tick, Is.EqualTo(-1));
            JObject initial = null;
            yield return Request(api, "/arena/inspection", null, value => initial = value);
            AssertProjection(initial, "native-initial");
            string application = owner.InspectStateJson();
            for (int index = 0; index < 3; index++)
            {
                JObject repeated = null;
                yield return Request(api, "/arena/inspection", null, value => repeated = value);
                Assert.That(JToken.DeepEquals(initial, repeated), Is.True, "GET must not change owner counters or game state");
                Assert.That(owner.InspectStateJson(), Is.EqualTo(application));
                Assert.That(client.State.tick, Is.EqualTo(-1));
            }
            Assert.That(client.Command("start"), Is.True);
            owner.Step();
            yield return Until(() => client.State.tick == 0, "manual start projection");
            JObject started = null;
            yield return Request(api, "/arena/inspection", null, value => started = value);
            AssertProjection(started, "native-start");
            Assert.That((bool)started["timing"]["last"]["runtimeAdvanced"], Is.True);
            Assert.That((bool)started["timing"]["last"]["simulationAdvanced"], Is.True);
            Assert.That(((JArray)started["events"]["entries"]).Any(item => (string)item["address"] == "/game/arena/run"), Is.True);
            Assert.That(client.Command("pause"), Is.True);
            owner.Step();
            yield return Until(() => client.State.tick == 1, "manual pause projection");
            JObject paused = null;
            yield return Request(api, "/arena/inspection", null, value => paused = value);
            AssertProjection(paused, "native-pause");
            Assert.That((bool)paused["timing"]["last"]["runtimeAdvanced"], Is.True);
            Assert.That((bool)paused["timing"]["last"]["simulationAdvanced"], Is.False);
            Assert.That((string)paused["epoch"], Is.EqualTo((string)started["epoch"]));
            Assert.That((string)paused["run"], Is.EqualTo((string)started["run"]));
            string output = owner.LastOutputJson;
            JObject afterRead = null;
            yield return Request(api, "/arena/inspection", null, value => afterRead = value);
            Assert.That(JToken.DeepEquals(paused, afterRead), Is.True);
            Assert.That(owner.LastOutputJson, Is.EqualTo(output), "inspection must not drain or add native output");
            SaveEvidence("native-inspection");
        }

        [UnityTest, Category("ArenaNetwork")]
        public IEnumerator NetworkInspectionMatchesRenderedReplayAcrossForwardBackwardSeekAndRetry()
        {
            string endpoint = Environment.GetEnvironmentVariable("KITU_ARENA_WS_URL");
            string recording = Environment.GetEnvironmentVariable("KITU_ARENA_REPLAY_ID");
            if (string.IsNullOrEmpty(endpoint) || string.IsNullOrEmpty(recording))
                Assert.Ignore("Set an isolated Arena endpoint and same-execution stock recording.");
            string api = new UriBuilder(endpoint) { Scheme = endpoint.StartsWith("wss:") ? "https" : "http", Path = "", Query = "" }.Uri.ToString().TrimEnd('/');
            yield return LoadClient(ArenaBackend.Server);
            yield return Request(api, "/arena/playback/command", new JObject { ["action"] = "live" });
            yield return Until(() => !client.ReplayActive, "live owner before replay");
            Assert.That(client.Command("menu"), Is.True);
            yield return Until(() => client.State.phase == 0, "fresh opening");
            Assert.That(client.Command("start"), Is.True);
            yield return Until(() => client.State.phase == 1, "parkable live run");
            yield return Request(api, "/arena/playback/load", new JObject { ["id"] = recording });
            yield return Until(() => client.ReplayActive && client.State.tick == -1, "verified replay initial projection");
            JObject inspected = null;
            yield return Request(api, "/arena/inspection", null, value => inspected = value);
            AssertProjection(inspected, "replay-initial");
            Assert.That((string)inspected["mode"]["recordingId"], Is.EqualTo(recording));
            ulong epoch = Counter(inspected["epoch"]);
            foreach (long tick in new long[] { 197, 1649, 5526, 5527, 197 })
            {
                yield return Request(api, "/arena/playback/seek", new JObject { ["tick"] = tick });
                yield return Until(() => client.State.tick == tick && client.Presentation.tick == tick, "replay seek " + tick);
                yield return Request(api, "/arena/inspection", null, value => inspected = value);
                AssertProjection(inspected, "replay-seek-" + tick);
                Assert.That(Counter(inspected["epoch"]), Is.GreaterThan(epoch));
                epoch = Counter(inspected["epoch"]);
                Assert.That((bool)inspected["readOnly"], Is.True);
                Assert.That((string)inspected["mode"]["tick"], Is.EqualTo(tick.ToString(CultureInfo.InvariantCulture)));
                if (tick == 197) Assert.That(client.Presentation.floor.offsetTick, Is.EqualTo(12));
                if (tick == 1649)
                {
                    var cue = client.Presentation.bosses.Single();
                    var ring = client.transform.Find("Arena presentation/tell-" + cue.entityId).GetComponent<LineRenderer>();
                    Assert.That(cue.offsetTick, Is.EqualTo(24));
                    Assert.That(ring.GetPosition(0).magnitude, Is.EqualTo(cue.radius).Within(1e-4));
                }
                if (tick == 5526) Assert.That(client.State.inventory.health, Is.EqualTo(0));
                if (tick == 5527) Assert.That(client.State.inventory.maxHealth, Is.EqualTo(100));
                var events = inspected["events"].DeepClone();
                yield return Request(api, "/arena/inspection", null, value => inspected = value);
                Assert.That(JToken.DeepEquals(events, inspected["events"]), Is.True, "idle replay does not duplicate events");
                Assert.That(client.State.tick, Is.EqualTo(tick));
            }
            yield return Request(api, "/arena/playback/command", new JObject { ["action"] = "step" });
            yield return Until(() => client.State.tick == 198, "single step after backward seek");
            yield return Request(api, "/arena/inspection", null, value => inspected = value);
            AssertProjection(inspected, "replay-step-198");
            Assert.That(Counter(inspected["epoch"]), Is.EqualTo(epoch), "step retains observation epoch");
            yield return Request(api, "/arena/playback/command", new JObject { ["action"] = "stop" });
            yield return Until(() => client.State.tick == -1, "stop returns initial state");
            yield return Request(api, "/arena/inspection", null, value => inspected = value);
            AssertProjection(inspected, "replay-stop");
            yield return Request(api, "/arena/playback/command", new JObject { ["action"] = "live" });
            yield return Until(() => !client.ReplayActive && client.State.overlay == "pause", "return to paused live run");
            SaveEvidence("network-inspection-" + (Environment.GetEnvironmentVariable("KITU_ARENA_ENCODING") ?? "default"));
        }

        private IEnumerator LoadClient(ArenaBackend backend)
        {
            evidence.Clear();
#if UNITY_EDITOR
            yield return UnityEditor.SceneManagement.EditorSceneManager.LoadSceneAsyncInPlayMode(
                "Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity", new LoadSceneParameters(LoadSceneMode.Single));
#else
            yield return SceneManager.LoadSceneAsync("KituEndlessArena", LoadSceneMode.Single);
#endif
            client = UnityEngine.Object.FindFirstObjectByType<KituArenaClient>();
            client.NativeConnection?.Dispose();
            client.ConnectOnStart = false;
            client.Backend = backend;
            client.DeviceInput = false;
            client.PauseOnFocusLoss = false;
            client.NativeAutomaticTicks = false;
            client.NativeBridgeEnabled = true;
            client.NativeBridgeAddress = "127.0.0.1:0";
            storage = Path.Combine(Path.GetTempPath(), "arena-inspection-test-" + Guid.NewGuid().ToString("N"));
            client.NativeStorageDirectory = storage;
            client.Connect();
            yield return Until(() => client.Connected, "inspection fixture connects");
        }

        private void AssertProjection(JObject snapshot, string label)
        {
            Assert.That((int)snapshot["schemaVersion"], Is.EqualTo(1));
            Assert.That((string)snapshot["sessionId"], Is.EqualTo(client.SessionId));
            var state = (JObject)snapshot["state"].DeepClone();
            Signed(state, "tick"); Unsigned(state, "simulationSteps");
            var expectedState = JsonUtility.FromJson<ArenaReferenceState>(state.ToString(Formatting.None));
            Assert.That(JsonUtility.ToJson(client.State), Is.EqualTo(JsonUtility.ToJson(expectedState)), label + " complete state");
            var presentation = (JObject)snapshot["presentation"].DeepClone();
            Signed(presentation, "tick"); Unsigned(presentation, "run"); Unsigned(presentation, "simulationStep");
            foreach (JObject cue in (JArray)presentation["bosses"]) { Signed(cue, "startedTick"); Unsigned(cue, "offsetTick"); }
            if (presentation["floor"] is JObject floor) { Signed(floor, "startedTick"); Unsigned(floor, "offsetTick"); }
            var expectedPresentation = ArenaPresentationState.FromJson(presentation.ToString(Formatting.None));
            Assert.That(JsonUtility.ToJson(client.Presentation), Is.EqualTo(JsonUtility.ToJson(expectedPresentation)), label + " complete presentation");
            var player = client.transform.Find("Arena presentation/Player");
            if (client.State.phase != 0)
            {
                Assert.That(player, Is.Not.Null);
                Assert.That(player.position.x, Is.EqualTo(client.State.playerPosition.x).Within(1e-4));
                Assert.That(player.position.z, Is.EqualTo(client.State.playerPosition.y).Within(1e-4));
            }
            evidence.Add(new JObject { ["label"] = label, ["inspection"] = snapshot.DeepClone(),
                ["unityState"] = JObject.Parse(JsonUtility.ToJson(client.State)),
                ["unityPresentation"] = JObject.Parse(JsonUtility.ToJson(client.Presentation)) });
        }

        private static void Signed(JObject value, string key)
        {
            Assert.That(value[key].Type, Is.EqualTo(JTokenType.String));
            value[key] = long.Parse((string)value[key], CultureInfo.InvariantCulture);
        }
        private static ulong Counter(JToken value)
        {
            Assert.That(value.Type, Is.EqualTo(JTokenType.String));
            return ulong.Parse((string)value, CultureInfo.InvariantCulture);
        }
        private static void Unsigned(JObject value, string key) { value[key] = Counter(value[key]); }

        private void SaveEvidence(string name)
        {
            string directory = Environment.GetEnvironmentVariable("KITU_ARENA_INSPECTION_EVIDENCE_DIR");
            if (string.IsNullOrEmpty(directory)) return;
            Assert.That(Path.IsPathRooted(directory), Is.True);
            Directory.CreateDirectory(directory);
            File.WriteAllText(Path.Combine(directory, name + ".json"), evidence.ToString(Formatting.Indented));
        }

        private static IEnumerator Request(string api, string path, JObject body, Action<JObject> receive = null)
        {
            using (var request = new UnityWebRequest(api.TrimEnd('/') + path, body == null ? "GET" : "POST"))
            {
                request.downloadHandler = new DownloadHandlerBuffer();
                if (body != null)
                {
                    request.uploadHandler = new UploadHandlerRaw(Encoding.UTF8.GetBytes(body.ToString(Formatting.None)));
                    request.SetRequestHeader("Content-Type", "application/json");
                }
                request.timeout = 60;
                yield return request.SendWebRequest();
                Assert.That(request.result, Is.EqualTo(UnityWebRequest.Result.Success), request.downloadHandler.text);
                if (body == null && path == "/arena/inspection")
                    Assert.That(request.GetResponseHeader("Cache-Control"), Does.Contain("no-store"));
                receive?.Invoke(JObject.Parse(request.downloadHandler.text));
            }
        }

        private IEnumerator Until(Func<bool> condition, string operation)
        {
            double deadline = Time.realtimeSinceStartupAsDouble + 60;
            while (!condition() && Time.realtimeSinceStartupAsDouble < deadline) yield return null;
            Assert.That(condition(), Is.True, operation + ": " + client?.Message);
        }
    }
}
