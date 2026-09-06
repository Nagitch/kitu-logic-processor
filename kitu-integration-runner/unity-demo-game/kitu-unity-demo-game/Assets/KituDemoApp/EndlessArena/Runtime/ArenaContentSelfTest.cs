using System;
using System.Collections;
using System.IO;
using System.Linq;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEngine;
using UnityEngine.Rendering;

namespace UnityOnlyArena
{
    // Opt-in, graphical Player bootstrap proof. This does not send gameplay
    // inputs or tick: the ordinary deterministic scenario verifier owns those.
    public sealed class ArenaContentSelfTest : MonoBehaviour
    {
        private KituArenaClient client;
        private string evidenceDirectory, expectedFailure;
        private readonly JObject result = new JObject();
        private int nativePeak, assetPeak;

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
        private static void Bootstrap()
        {
            if (!Environment.GetCommandLineArgs().Contains("--arena-content-self-test")) return;
            string evidence = null;
            ArenaContentSelfTest test = null;
            try
            {
                if (Environment.GetCommandLineArgs().Contains("--arena-self-test"))
                    throw new ArgumentException("Content bootstrap proof and gameplay self-test are mutually exclusive");
                evidence = ArenaLaunchArguments.Value("--arena-content-evidence");
                if (string.IsNullOrEmpty(evidence) || !Path.IsPathRooted(evidence))
                    throw new ArgumentException("Content proof requires an absolute --arena-content-evidence directory");
                string expected = ArenaLaunchArguments.Value("--arena-content-expect-failure");
                if (expected != null && string.IsNullOrWhiteSpace(expected))
                    throw new ArgumentException("Expected content failure must name a nonempty diagnostic substring");
                Directory.CreateDirectory(evidence);
                test = new GameObject("Arena content bootstrap verification").AddComponent<ArenaContentSelfTest>();
                test.evidenceDirectory = Path.GetFullPath(evidence);
                test.expectedFailure = expected;
                test.client = FindAnyObjectByType<KituArenaClient>();
                if (test.client == null) throw new InvalidOperationException("Kitu scene has no Arena client");
                test.client.ConnectOnStart = false;
                test.client.DeviceInput = false;
                test.client.PauseOnFocusLoss = false;
                test.client.Backend = ArenaBackend.Embedded;
                test.client.NativeBridgeEnabled = false;
                test.client.NativeAutomaticTicks = false;
                if (ArenaNativeConnection.LiveHandleCount != 0)
                    throw new InvalidOperationException("Runtime was created before content probe configuration");
                test.client.Connect();
            }
            catch (Exception error)
            {
                if (test != null) test.enabled = false;
                Debug.LogError("Arena content probe could not start: " + error);
                foreach (var candidate in FindObjectsByType<KituArenaClient>(FindObjectsSortMode.None)) candidate.gameObject.SetActive(false);
                if (!string.IsNullOrEmpty(evidence) && Path.IsPathRooted(evidence))
                {
                    try
                    {
                        Directory.CreateDirectory(evidence);
                        File.WriteAllText(Path.Combine(evidence, "content-result.json"), new JObject {
                            ["schemaVersion"] = 1, ["matched"] = false, ["error"] = error.ToString()
                        }.ToString(Formatting.Indented));
                    }
                    catch (Exception writeError) { Debug.LogError(writeError); }
                }
                Application.Quit(1);
            }
        }

        private IEnumerator Start()
        {
            // Catch iterator failures without bypassing cleanup or relying on
            // Unity to turn an assertion log into a failing process exit code.
            Exception failure = null;
            var probe = Probe();
            try
            {
                while (true)
                {
                    bool more = false;
                    object value = null;
                    try { more = probe.MoveNext(); if (more) value = probe.Current; }
                    catch (Exception error) { failure = error; }
                    if (failure != null || !more) break;
                    yield return value;
                }
            }
            finally { (probe as IDisposable)?.Dispose(); }
            if (failure != null) result["error"] = failure.ToString();
            try { if (client != null) client.gameObject.SetActive(false); }
            catch (Exception error) { failure = failure ?? error; result["cleanupError"] = error.ToString(); }
            // The native owner closes synchronously; cloned Unity objects and
            // their asset leases retire at the next frame boundary.
            result["nativeHandlesAfterDisable"] = ArenaNativeConnection.LiveHandleCount;
            yield return null;
            yield return null;
            result["nativeHandlesAfterCleanup"] = ArenaNativeConnection.LiveHandleCount;
            result["assetHandlesAfterCleanup"] = ArenaAddressableAssets.LiveHandleCount;
            bool cleaned = ArenaNativeConnection.LiveHandleCount == 0 && ArenaAddressableAssets.LiveHandleCount == 0;
            result["matched"] = failure == null && cleaned;
            if (!cleaned && result["error"] == null) result["error"] = "Content probe did not release all owned Runtime/asset handles";
            bool written = false;
            try
            {
                File.WriteAllText(Path.Combine(evidenceDirectory, "content-result.json"), result.ToString(Formatting.Indented));
                written = true;
            }
            catch (Exception error) { Debug.LogError("Cannot write Arena content evidence: " + error); }
            bool passed = written && (bool)result["matched"];
            if (passed) Debug.Log("Arena content bootstrap proof passed: " + result.ToString(Formatting.None));
            else Debug.LogError("Arena content bootstrap proof failed: " + result.ToString(Formatting.None));
            Application.Quit(passed ? 0 : 1);
        }

        private IEnumerator Probe()
        {
            result["schemaVersion"] = 1;
            result["expectedFailure"] = expectedFailure == null ? JValue.CreateNull() : new JValue(expectedFailure);
            result["streamingAssetsPath"] = Application.streamingAssetsPath;
            result["applicationDataPath"] = Application.dataPath;
            result["persistentDataPath"] = Application.persistentDataPath;
            result["workingDirectory"] = Directory.GetCurrentDirectory();
            result["packageDirectory"] = ArenaLaunchArguments.Value("--arena-package") ??
                (string.IsNullOrEmpty(client.BundledContentDirectory) ? Path.Combine(Application.streamingAssetsPath, "KituArena") : client.BundledContentDirectory);
            Require(!Application.isBatchMode && SystemInfo.graphicsDeviceType != GraphicsDeviceType.Null,
                "Content bootstrap proof requires an actual graphical Player");
            double deadline = Time.realtimeSinceStartupAsDouble + 45;
            while (!client.Connected && string.IsNullOrEmpty(client.PreparationError) &&
                !client.Message.StartsWith("Native creation failed", StringComparison.Ordinal))
            {
                Observe();
                Require(Time.realtimeSinceStartupAsDouble < deadline, "Content preparation timed out: " + client.Message);
                yield return null;
            }
            // Make one-shot preparation errors and unexpected automatic ticking
            // observable after several normal Update opportunities.
            for (int frame = 0; frame < 4; frame++) { Observe(); yield return null; }
            Observe();
            string diagnostic = client.PreparationError ?? client.Message;
            result["diagnostic"] = diagnostic;
            result["prepared"] = client.AssetsReady;
            result["connected"] = client.Connected;
            result["tick"] = client.State.tick;
            result["nativeHandles"] = ArenaNativeConnection.LiveHandleCount;
            result["assetHandles"] = ArenaAddressableAssets.LiveHandleCount;
            result["nativeHandlePeak"] = nativePeak;
            result["assetHandlePeak"] = assetPeak;
            result["assetReport"] = client.AssetReport?.DeepClone();
            result["referenceGameCount"] = FindObjectsByType<ArenaGame>(FindObjectsSortMode.None).Length;
            result["presentationCount"] = client.GetComponentsInChildren<Transform>(true).Count(value => value.name == "Arena presentation");
            Require(client.State.tick == -1, "Bootstrap must not execute a game/management tick");
            Require((int)result["referenceGameCount"] == 0, "Kitu content path constructed the procedural reference game");
            if (expectedFailure != null)
            {
                Require(!client.Connected && client.NativeConnection == null && nativePeak == 0,
                    "Expected initialization failure created an embedded Runtime");
                Require(!string.IsNullOrEmpty(diagnostic) && diagnostic.IndexOf(expectedFailure, StringComparison.OrdinalIgnoreCase) >= 0,
                    "Initialization did not report the expected cause: " + expectedFailure);
                Require(!client.AssetsReady && client.GameCamera == null && (int)result["presentationCount"] == 0,
                    "Failed asset/package preparation retained a fallback presentation");
                yield break;
            }
            Require(client.AssetsReady && client.Connected && client.NativeConnection != null,
                "Content did not prepare and synchronize: " + diagnostic);
            Require(nativePeak == 1 && ArenaNativeConnection.LiveHandleCount == 1,
                "Content bootstrap must create exactly one embedded Runtime");
            Require(ArenaAddressableAssets.LiveHandleCount == 4, "Content bootstrap must retain exactly four asset leases");
            Require(client.NativeConnection.BridgeEndpoint == null, "Content probe unexpectedly enabled the development bridge");
            Require((int)result["presentationCount"] == 1 && client.GameCamera != null, "Content bootstrap did not create exactly one view");
            var hostBundles = JArray.Parse(client.NativeConnection.InspectHostJson());
            result["nativeHostBundles"] = hostBundles;
            JObject status = null;
            foreach (JObject bundle in hostBundles)
                foreach (JObject message in (JArray)bundle["messages"])
                    if ((string)message["address"] == "/host/arena/status") status = JObject.Parse((string)message["args"][0]["value"]);
            Require(status != null, "Embedded Runtime has no host inspection status");
            result["nativeHost"] = status;
            Require((string)status["package"]?["hash"] == (string)client.AssetReport?["package"]?["identity"] && status["package"] != null,
                "Unity assets and embedded game did not initialize from the same package identity");
            result["initialState"] = JArray.Parse(client.NativeConnection.InspectStateJson());
        }

        private void Observe()
        {
            nativePeak = Math.Max(nativePeak, ArenaNativeConnection.LiveHandleCount);
            assetPeak = Math.Max(assetPeak, ArenaAddressableAssets.LiveHandleCount);
        }
        private static void Require(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
    }
}
