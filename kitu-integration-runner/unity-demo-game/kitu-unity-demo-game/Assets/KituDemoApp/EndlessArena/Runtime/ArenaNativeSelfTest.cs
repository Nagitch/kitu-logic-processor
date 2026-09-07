using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEngine;
using UnityEngine.Rendering;

namespace UnityOnlyArena
{
    /// <summary>
    /// Opt-in graphical Player verification. Replays recorded native inputs and
    /// compares the complete native output with Rust Runtime observations. It
    /// contains no game model and never patches application state.
    /// </summary>
    public sealed class ArenaNativeSelfTest : MonoBehaviour
    {
        private string tracePath, expectedPath, evidenceDirectory, scenario, initialExpectedPath;
        private KituArenaClient client;
        private long ticks;
        private ulong inputs;
        private readonly HashSet<string> screenshots = new HashSet<string>();
        private bool deathObserved, retryObserved, finished;
        private int scriptTelegraphTicks;
        private int timelinePausedTicks;
        private JObject initialReport;

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
        private static void Bootstrap()
        {
            try
            {
                string trace = ArenaLaunchArguments.Value("--arena-self-test");
                if (trace == null) return;
                string expected = ArenaLaunchArguments.Value("--arena-expected");
                string evidence = ArenaLaunchArguments.Value("--arena-evidence");
                if (string.IsNullOrEmpty(expected) || string.IsNullOrEmpty(evidence))
                    throw new ArgumentException("Native self-test needs --arena-expected and --arena-evidence");
                var test = new GameObject("Native Arena Player verification").AddComponent<ArenaNativeSelfTest>();
                test.tracePath = Path.GetFullPath(trace);
                test.scenario = Path.GetFileNameWithoutExtension(test.tracePath);
                if (test.scenario != "preparation" && test.scenario != "stock-eleven-death-retry" && test.scenario != "rhai-boss" && test.scenario != "timeline-cues" && test.scenario != "bundled-edited")
                    throw new ArgumentException("Unknown native Arena self-test scenario");
                test.initialExpectedPath = ArenaLaunchArguments.Value("--arena-initial-expected");
                if (test.scenario == "bundled-edited" && string.IsNullOrEmpty(test.initialExpectedPath))
                    throw new ArgumentException("Bundled source proof requires --arena-initial-expected");
                test.expectedPath = Path.GetFullPath(expected);
                test.evidenceDirectory = Path.GetFullPath(evidence);
                Directory.CreateDirectory(test.evidenceDirectory);
                test.client = FindAnyObjectByType<KituArenaClient>();
                if (test.client == null) throw new InvalidOperationException("Kitu scene has no Arena client");
                test.client.ConnectOnStart = false;
                test.client.DeviceInput = false;
                test.client.PauseOnFocusLoss = false;
                test.client.Backend = ArenaBackend.Embedded;
                test.client.NativeBridgeEnabled = false;
                test.client.NativeAutomaticTicks = false;
                test.client.NativeConnection?.Dispose();
                test.client.Connect();
            }
            catch (Exception error)
            {
                Debug.LogError("Native Arena self-test could not start: " + error);
                Application.Quit(1);
            }
        }

        private IEnumerator Start()
        {
            var stack = new Stack<IEnumerator>();
            stack.Push(Run());
            while (stack.Count > 0)
            {
                bool more = false;
                object value = null;
                Exception failure = null;
                try
                {
                    more = stack.Peek().MoveNext();
                    if (more) value = stack.Peek().Current;
                    else (stack.Pop() as IDisposable)?.Dispose();
                }
                catch (Exception error) { failure = error; }
                if (failure != null)
                {
                    while (stack.Count > 0) (stack.Pop() as IDisposable)?.Dispose();
                    Finish(false, failure.ToString());
                    yield break;
                }
                if (!more) continue;
                if (value is IEnumerator nested) stack.Push(nested);
                else yield return value;
            }
        }

        private IEnumerator Run()
        {
            if (Application.isBatchMode || SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
                throw new InvalidOperationException("Native Player self-test requires a graphical launch");
            double deadline = Time.realtimeSinceStartupAsDouble + 10;
            while (!client.Connected)
            {
                if (Time.realtimeSinceStartupAsDouble > deadline)
                    throw new InvalidOperationException("Native client did not synchronize: " + client.Message);
                yield return null;
            }
            var native = client.NativeConnection;
            if (native == null || client.State.tick != -1 || native.BridgeEndpoint != null)
                throw new InvalidOperationException("Self-test must start on an unadvanced embedded Runtime with its bridge disabled");
            if (FindAnyObjectByType<ArenaGame>() != null)
                throw new InvalidOperationException("Unity-only game logic must not run in the Kitu scene");
            if (!client.AssetsReady || ((JArray)client.AssetReport["loaded"]).Count != 4)
                throw new InvalidOperationException("Player must use all four actual Addressable assets");
            JObject host = null;
            foreach (JObject bundle in JArray.Parse(native.InspectHostJson()))
                foreach (JObject message in (JArray)bundle["messages"])
                    if ((string)message["address"] == "/host/arena/status") host = JObject.Parse((string)message["args"][0]["value"]);
            if (host?["package"] == null || (string)host["package"]["hash"] != (string)client.AssetReport["package"]["identity"])
                throw new InvalidOperationException("Player game and assets must use the same bundled package");
            initialReport = new JObject { ["state"] = JArray.Parse(native.InspectStateJson()), ["package"] = host["package"].DeepClone() };
            if (initialExpectedPath != null && !JToken.DeepEquals(JObject.Parse(File.ReadAllText(initialExpectedPath)), initialReport))
                throw new InvalidOperationException("Packaged initial state or source versions differ before tick0");
            File.WriteAllText(Path.Combine(evidenceDirectory, "initial.json"), initialReport.ToString(Formatting.Indented));
            using (var trace = new StreamReader(tracePath))
            using (var expected = new StreamReader(expectedPath))
            using (var actual = new StreamWriter(Path.Combine(evidenceDirectory, "actual.ndjson")))
            {
                string line;
                while ((line = trace.ReadLine()) != null)
                {
                    if (line.StartsWith("I", StringComparison.Ordinal))
                    {
                        ulong sequence = native.SubmitInputJson(line.Substring(1));
                        if (sequence != inputs) throw new InvalidOperationException("Native input order mismatch at " + inputs);
                        inputs++;
                        continue;
                    }
                    if (line != "T") throw new InvalidDataException("Invalid native trace record");
                    string expectedLine = expected.ReadLine();
                    if (expectedLine == null) throw new InvalidDataException("Expected observations ended before trace");
                    var target = JObject.Parse(expectedLine);
                    native.Step();
                    var observed = new JObject {
                        ["tick"] = ticks,
                        ["output"] = JArray.Parse(native.LastOutputJson),
                        ["state"] = JArray.Parse(native.InspectStateJson()),
                    };
                    actual.WriteLine(observed.ToString(Formatting.None));
                    if (!JToken.DeepEquals(target, observed))
                    {
                        actual.Flush();
                        File.WriteAllText(Path.Combine(evidenceDirectory, "mismatch-expected.json"), target.ToString());
                        File.WriteAllText(Path.Combine(evidenceDirectory, "mismatch-actual.json"), observed.ToString());
                        throw new InvalidOperationException("Native state or output differs from the Rust Runtime at tick " + ticks);
                    }
                    // Only delivery/rendering advances here. Native automatic
                    // ticking is disabled, so captures never skip a recorded tick.
                    yield return null;
                    if (client.State.tick != ticks)
                        throw new InvalidOperationException("Unity projection has not received native tick " + ticks);
                    AssertRenderedPlayer();
                    AssertRenderedPresentation();
                    string checkpoint = Checkpoint();
                    if (checkpoint != null && !screenshots.Contains(checkpoint)) yield return Capture(checkpoint);
                    ticks++;
                }
                if (expected.ReadLine() != null) throw new InvalidDataException("Expected observations contain unexecuted ticks");
            }
            bool stock = scenario == "stock-eleven-death-retry";
            if (stock && (ticks != 5528 || inputs != 5581 || !deathObserved || !retryObserved))
                throw new InvalidOperationException("Stock verification must include all 5528 ticks, 5581 inputs, natural 11F death and retry");
            bool script = scenario == "rhai-boss";
            bool timeline = scenario == "timeline-cues";
            bool bundled = scenario == "bundled-edited";
            if ((script || bundled) && (ticks != 1800 || scriptTelegraphTicks != 96))
                throw new InvalidOperationException("Script verification must include 1800 ticks and exactly 96 telegraph ticks");
            if (timeline && (ticks != 1830 || inputs != 1819 || timelinePausedTicks != 30))
                throw new InvalidOperationException("Timeline verification requires 1830 ticks, 1819 inputs and 30 frozen cue ticks");
            if (!stock && !script && !timeline && !bundled && (ticks != 28 || inputs != 40))
                throw new InvalidOperationException("Preparation verification must include all 28 ticks and 40 inputs");
            foreach (string name in stock ? new[] { "chest", "combat", "boss", "death-11f", "retry" } : script ? new[] { "boss-script" } : timeline ? new[] { "timeline-floor", "timeline-boss", "timeline-paused" } : bundled ? new[] { "bundled-weapon", "bundled-floor", "bundled-boss" } : new[] { "inventory" })
                if (!screenshots.Contains(name)) throw new InvalidOperationException("Missing rendered checkpoint " + name);
            Finish(true, null);
        }

        private string Checkpoint()
        {
            var state = client.State;
            if (scenario == "bundled-edited")
            {
                var boss = state.enemies.FirstOrDefault(enemy => enemy.Kind == ArenaEnemyKind.Boss && enemy.BossState == ArenaBossState.Telegraph);
                if (boss != null)
                {
                    if (scriptTelegraphTicks == 0 && Mathf.Abs(boss.PhaseRemaining - 1.6f) > 1e-6f)
                        throw new InvalidOperationException("Bundled Rhai must govern the first boss telegraph");
                    scriptTelegraphTicks++;
                }
                if (state.inventory.equipment.Any(item => item.kind == 0 && item.damage == 32))
                {
                    if (!screenshots.Contains("bundled-weapon")) return "bundled-weapon";
                }
                if (client.Presentation.floor != null && client.Presentation.floor.offsetTick == 12)
                {
                    if (Mathf.Abs(client.Presentation.floor.opacity - .85f) > 1e-6f) throw new InvalidOperationException("Bundled floor cue was not applied");
                    return "bundled-floor";
                }
                var cue = client.Presentation.bosses.FirstOrDefault(value => value.offsetTick == 24);
                if (cue != null)
                {
                    if (Mathf.Abs(cue.radius - 4.5f) > 1e-6f) throw new InvalidOperationException("Bundled boss cue was not applied");
                    return "bundled-boss";
                }
                return null;
            }
            if (scenario == "timeline-cues")
            {
                var presentation = client.Presentation;
                if (presentation.floor != null && presentation.floor.toFloor == 1 && presentation.floor.offsetTick == 12)
                {
                    if (Mathf.Abs(presentation.floor.opacity - .85f) > 1e-6f)
                        throw new InvalidOperationException("Edited floor timeline opacity did not reach the renderer");
                    return "timeline-floor";
                }
                var cue = presentation.bosses.FirstOrDefault(value => value.offsetTick == 24);
                if (cue != null)
                {
                    if (Mathf.Abs(cue.radius - 4.5f) > 1e-6f)
                        throw new InvalidOperationException("Edited boss timeline radius did not reach the renderer");
                    if (state.overlay == "pause") { timelinePausedTicks++; return "timeline-paused"; }
                    return "timeline-boss";
                }
            }
            if (scenario == "rhai-boss" && state.floor == 5)
            {
                var boss = state.enemies.FirstOrDefault(enemy => enemy.Kind == ArenaEnemyKind.Boss && enemy.BossState == ArenaBossState.Telegraph);
                if (boss != null)
                {
                    if (scriptTelegraphTicks == 0 && Mathf.Abs(boss.PhaseRemaining - 1.6f) > 1e-6f)
                        throw new InvalidOperationException("Edited script telegraph must begin at 1.6 seconds");
                    scriptTelegraphTicks++;
                    return "boss-script";
                }
            }
            if (scenario == "preparation" && state.overlay == "inventory") return "inventory";
            if (state.overlay == "chest") return "chest";
            if (state.floor == 1 && state.phase == (int)ArenaPhase.Combat) return "combat";
            if (state.enemies.Any(enemy => enemy.Kind == ArenaEnemyKind.Boss && enemy.BossState == ArenaBossState.Telegraph)) return "boss";
            if (state.phase == (int)ArenaPhase.Results && state.floor == 11)
            {
                if (state.tick != 5526 || state.inventory.health != 0 || state.floorsCleared != 10 || state.bossesDefeated != 2)
                    throw new InvalidOperationException("Native stock death projection is incomplete");
                deathObserved = true;
                return "death-11f";
            }
            if (deathObserved && state.phase == (int)ArenaPhase.Preparing && state.floor == 0)
            {
                if (state.tick != 5527 || state.inventory.health != 100 || state.inventory.maxHealth != 100 || state.result.present)
                    throw new InvalidOperationException("Native retry did not reset run state");
                retryObserved = true;
                return "retry";
            }
            return null;
        }

        private void AssertRenderedPlayer()
        {
            if (client.GameCamera == null) throw new InvalidOperationException("Kitu scene has no presentation camera");
            // The frozen preparation trace returns to Opening, where the
            // presentation hierarchy remains valid but is intentionally hidden.
            var player = client.transform.Find("Arena presentation/Player");
            if (player == null) throw new InvalidOperationException("Kitu scene has no rendered player");
            bool inRun = client.State.phase != (int)ArenaPhase.Opening;
            if (player.gameObject.activeInHierarchy != inRun)
                throw new InvalidOperationException("Rendered player visibility differs from the native phase");
            Vector3 position = player.position;
            if (Mathf.Abs(position.x - client.State.playerPosition.x) > 1e-4f ||
                Mathf.Abs(position.z - client.State.playerPosition.y) > 1e-4f)
                throw new InvalidOperationException("Rendered player differs from the native projection");
        }

        private void AssertRenderedPresentation()
        {
            var presentation = client.Presentation;
            if (presentation == null || presentation.contractVersion != 1 || presentation.tick != client.State.tick)
                throw new InvalidOperationException("Unity has not received the matching Runtime presentation snapshot");
            foreach (var cue in presentation.bosses)
            {
                var tell = client.transform.Find("Arena presentation/tell-" + cue.entityId);
                if (cue.intensity <= 0) continue;
                if (tell == null || !tell.gameObject.activeInHierarchy)
                    throw new InvalidOperationException("Timeline boss cue has no visible ring");
                var ring = tell.GetComponent<LineRenderer>();
                if (Mathf.Abs(ring.GetPosition(0).magnitude - cue.radius) > 1e-4f ||
                    Mathf.Abs(ring.startWidth - (.025f + cue.intensity * .1f)) > 1e-6f)
                    throw new InvalidOperationException("Rendered boss cue differs from the Runtime timeline values");
            }
        }

        private IEnumerator Capture(string name)
        {
            yield return null;
            yield return null;
            string path = Path.Combine(evidenceDirectory, name + ".png");
            if (File.Exists(path)) File.Delete(path);
            ScreenCapture.CaptureScreenshot(path);
            double deadline = Time.realtimeSinceStartupAsDouble + 5;
            while (!CompletePng(path))
            {
                if (Time.realtimeSinceStartupAsDouble > deadline)
                    throw new IOException("No completed screenshot for " + name);
                yield return null;
            }
            screenshots.Add(name);
            Debug.Log("Native Arena rendered checkpoint " + name + " at tick " + client.State.tick);
        }

        private static bool CompletePng(string path)
        {
            if (!File.Exists(path)) return false;
            try
            {
                using (var file = File.Open(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
                {
                    if (file.Length < 32 || file.ReadByte() != 137 || file.ReadByte() != 80 || file.ReadByte() != 78 || file.ReadByte() != 71) return false;
                    file.Seek(-12, SeekOrigin.End);
                    var tail = new byte[12];
                    return file.Read(tail, 0, tail.Length) == tail.Length && tail[4] == 73 && tail[5] == 69 && tail[6] == 78 && tail[7] == 68;
                }
            }
            catch (IOException) { return false; }
        }

        private void Finish(bool matched, string error)
        {
            if (finished) return;
            finished = true;
            var result = new JObject {
                ["backend"] = "native", ["scenario"] = scenario, ["matched"] = matched, ["ticks"] = ticks, ["inputs"] = inputs,
                ["sessionId"] = client?.SessionId, ["deathObserved"] = deathObserved, ["retryObserved"] = retryObserved,
                ["screenshots"] = new JArray(screenshots.OrderBy(name => name).Select(name => name + ".png")),
                ["error"] = error,
                ["initial"] = initialReport, ["assets"] = client?.AssetReport?.DeepClone(),
            };
            File.WriteAllText(Path.Combine(evidenceDirectory, "result.json"), result.ToString(Formatting.Indented));
            if (matched) Debug.Log("Native Arena Player verification passed: " + result.ToString(Formatting.None));
            else Debug.LogError("Native Arena Player verification failed: " + error);
            Application.Quit(matched ? 0 : 1);
        }
    }
}
