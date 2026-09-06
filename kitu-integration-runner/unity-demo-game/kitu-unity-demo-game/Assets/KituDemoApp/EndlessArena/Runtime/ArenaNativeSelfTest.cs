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
        private string tracePath, expectedPath, evidenceDirectory, scenario;
        private KituArenaClient client;
        private long ticks;
        private ulong inputs;
        private readonly HashSet<string> screenshots = new HashSet<string>();
        private bool deathObserved, retryObserved, finished;

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
                if (test.scenario != "preparation" && test.scenario != "stock-eleven-death-retry")
                    throw new ArgumentException("Native self-test requires a frozen preparation or stock-eleven-death-retry trace");
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
                    string checkpoint = Checkpoint();
                    if (checkpoint != null && !screenshots.Contains(checkpoint)) yield return Capture(checkpoint);
                    ticks++;
                }
                if (expected.ReadLine() != null) throw new InvalidDataException("Expected observations contain unexecuted ticks");
            }
            bool stock = scenario == "stock-eleven-death-retry";
            if (stock && (ticks != 5528 || inputs != 5581 || !deathObserved || !retryObserved))
                throw new InvalidOperationException("Stock verification must include all 5528 ticks, 5581 inputs, natural 11F death and retry");
            if (!stock && (ticks != 28 || inputs != 40))
                throw new InvalidOperationException("Preparation verification must include all 28 ticks and 40 inputs");
            foreach (string name in stock ? new[] { "chest", "combat", "boss", "death-11f", "retry" } : new[] { "inventory" })
                if (!screenshots.Contains(name)) throw new InvalidOperationException("Missing rendered checkpoint " + name);
            Finish(true, null);
        }

        private string Checkpoint()
        {
            var state = client.State;
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
            };
            File.WriteAllText(Path.Combine(evidenceDirectory, "result.json"), result.ToString(Formatting.Indented));
            if (matched) Debug.Log("Native Arena Player verification passed: " + result.ToString(Formatting.None));
            else Debug.LogError("Native Arena Player verification failed: " + error);
            Application.Quit(matched ? 0 : 1);
        }
    }
}
