using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace UnityOnlyArena
{
    /// <summary>
    /// Owns one native application on Unity's main thread. Attachment can be
    /// closed and reopened independently of the application's lifetime.
    /// </summary>
    public sealed class ArenaNativeConnection : IArenaConnection
    {
        private const int Ok = 0, Empty = 1, BufferTooSmall = 2;
        private const int MaximumBytes = 64 * 1024 * 1024;
        private static readonly List<ArenaNativeConnection> live = new List<ArenaNativeConnection>();
        private readonly int ownerThread = Thread.CurrentThread.ManagedThreadId;
        private readonly Queue<string> incoming = new Queue<string>();
        private readonly ArenaTickClock clock = new ArenaTickClock();
        private IntPtr handle;
        private bool attached, faulted;
        private ulong nextControlId = 1;
        private string sessionId, previousMode;
        public bool Connected => handle != IntPtr.Zero && attached && !faulted;
        public string Status { get; private set; } = "Creating embedded Runtime";
        public bool AutomaticTicks { get; set; } = true;
        public bool ReadOnly { get; private set; }
        public string SessionId => sessionId;
        public string BridgeEndpoint { get; private set; }
        public string LastOutputJson { get; private set; } = "[]";
        public double PendingSeconds => clock.PendingSeconds;
        public bool IsDisposed => handle == IntPtr.Zero;
        public static int LiveHandleCount => live.Count;

        public ArenaNativeConnection(bool bridgeEnabled, string bridgeAddress, string storageDirectory,
            string contentPath = null, string scriptPath = null)
        {
            if (Application.platform != RuntimePlatform.OSXEditor && Application.platform != RuntimePlatform.OSXPlayer)
                throw new PlatformNotSupportedException("The embedded Arena library currently supports macOS.");
            if (Native.AbiVersion() != 1) throw new InvalidOperationException("Incompatible Kitu native ABI");
            var config = new JObject {
                ["bridge"] = new JObject { ["enabled"] = bridgeEnabled, ["address"] = bridgeAddress },
            };
            if (!string.IsNullOrEmpty(storageDirectory)) config["storageDirectory"] = storageDirectory;
            if (!string.IsNullOrEmpty(contentPath)) config["contentPath"] = contentPath;
            if (!string.IsNullOrEmpty(scriptPath)) config["scriptPath"] = scriptPath;
            byte[] bytes = Encoding.UTF8.GetBytes(config.ToString(Formatting.None));
            var error = new byte[4096];
            int status = Native.Create(1, bytes, Size(bytes.Length), out handle, error, Size(error.Length), out UIntPtr required);
            if (status != Ok)
            {
                string diagnostic = required.ToUInt64() <= (ulong)error.Length
                    ? Encoding.UTF8.GetString(error, 0, (int)required.ToUInt64())
                    : "Creation diagnostic exceeded 4096 bytes (required " + required.ToUInt64() + ")";
                throw new InvalidOperationException("Native creation failed (" + status + "): " + diagnostic);
            }
            if (handle == IntPtr.Zero) throw new InvalidOperationException("Native creation returned no application");
            live.Add(this);
            try { Reconnect(); }
            catch { Dispose(); throw; }
        }

        public void Reconnect()
        {
            EnsureOwner();
            EnsureUsable();
            incoming.Clear();
            previousMode = null;
            // Force a complete session/projection handshake without recreating the run.
            sessionId = null;
            attached = true;
            RefreshHost();
            QueueBundles(JArray.Parse(InspectStateJson()));
            Status = "Embedded Runtime";
        }

        public void Disconnect()
        {
            EnsureOwner();
            if (handle == IntPtr.Zero || !attached) return;
            if (!faulted)
            {
                RefreshHost();
                if (ReadOnly)
                {
                    attached = false;
                    Status = "Embedded replay observer disconnected; Runtime retained";
                    return;
                }
                var request = new JObject {
                    ["metadata"] = new JObject { ["source"] = "host:native-controller",
                        ["messageId"] = nextControlId++, ["schemaVersion"] = 1 },
                    ["bundle"] = new JObject { ["messages"] = new JArray(new JObject {
                        ["address"] = "/input/arena/disconnect", ["args"] = new JArray(),
                    }) },
                };
                try { Submit(request.ToString(Formatting.None)); }
                catch (NativeFailure error) when (error.Code == -8)
                {
                    // An HTTP replay request may become read-only after the
                    // host inspection. Its activation already parks live input.
                    // Detachment must still finish without discarding the owner.
                    Queue(new JObject { ["type"] = "error", ["message"] = error.Message });
                }
            }
            attached = false;
            Status = "Embedded controller disconnected; Runtime retained";
        }

        public bool Send(string message)
        {
            EnsureOwner();
            if (!Connected) return false;
            try
            {
                RefreshHost();
                if (ReadOnly) return false;
                var envelope = JObject.Parse(message);
                if ((string)envelope["sessionId"] != sessionId || (int?)envelope["schemaVersion"] != 1)
                    throw new InvalidOperationException("Native Arena session or contract changed; reconnect before sending input");
                string source = (string)envelope["clientId"];
                string address = (string)envelope["address"];
                if (source == null || source.StartsWith("host:", StringComparison.Ordinal) || address == "/input/arena/disconnect")
                    throw new InvalidOperationException("Reserved native controller identity or command");
                var request = new JObject {
                    ["metadata"] = new JObject { ["source"] = source, ["messageId"] = envelope["messageId"], ["schemaVersion"] = 1 },
                    ["bundle"] = new JObject { ["messages"] = new JArray(new JObject {
                        ["address"] = address, ["args"] = envelope["args"],
                    }) },
                };
                Submit(request.ToString(Formatting.None));
                return true;
            }
            catch (NativeFailure error) when (error.Code == -2 || error.Code == -7 || error.Code == -8)
            {
                // Admission refusal leaves the native queue/state usable. In
                // particular replay activation can race the preceding snapshot.
                Queue(new JObject { ["type"] = "error", ["message"] = error.Message });
                return false;
            }
        }

        /// <summary>Submits a recorded typed request to the same native admission queue.</summary>
        public ulong SubmitInputJson(string request)
        {
            EnsureOwner();
            EnsureUsable();
            if (!attached) throw new InvalidOperationException("Native controller is detached");
            RefreshHost();
            if (ReadOnly) throw new InvalidOperationException("Replay is read-only");
            return Submit(request);
        }

        private ulong Submit(string request)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(request);
            Check(Native.Submit(handle, bytes, Size(bytes.Length), out ulong sequence));
            return sequence;
        }

        public bool TryReceive(out string message)
        {
            EnsureOwner();
            if (incoming.Count == 0) { message = null; return false; }
            message = incoming.Dequeue();
            return true;
        }

        public void Pump(double elapsedSeconds)
        {
            EnsureOwner();
            if (handle == IntPtr.Zero || faulted) return;
            try
            {
                RefreshHost();
                if (AutomaticTicks) clock.Advance(elapsedSeconds, Step);
            }
            catch (Exception error)
            {
                faulted = true;
                attached = false;
                Status = "Embedded Runtime stopped: " + error.Message;
                incoming.Clear();
                Queue(new JObject { ["type"] = "error", ["message"] = Status });
            }
        }

        /// <summary>Advances one fixed native tick; used by deterministic Player verification.</summary>
        public void Step()
        {
            EnsureOwner();
            EnsureUsable();
            Check(Native.Tick(handle));
            LastOutputJson = Read(Native.ReadOutput);
            QueueBundles(JArray.Parse(LastOutputJson));
            RefreshHost();
        }

        public string InspectStateJson()
        {
            EnsureOwner();
            EnsureUsable();
            return Read(Native.Inspect);
        }

        private void RefreshHost()
        {
            var bundles = JArray.Parse(Read(Native.InspectHost));
            JObject status = null;
            foreach (JObject bundle in bundles)
                foreach (JObject message in (JArray)bundle["messages"])
                    if ((string)message["address"] == "/host/arena/status")
                        status = JObject.Parse((string)message["args"][0]["value"]);
            if (status == null || (int?)status["schemaVersion"] != 1 || string.IsNullOrEmpty((string)status["sessionId"]))
                throw new InvalidOperationException("Missing or incompatible embedded Arena host metadata");
            if ((bool?)status["closing"] == true) throw new InvalidOperationException("Embedded Arena host is closing");
            string nextSession = (string)status["sessionId"];
            if (nextSession != sessionId)
            {
                sessionId = nextSession;
                Queue(new JObject { ["type"] = "arenaSession", ["id"] = sessionId, ["schemaVersion"] = 1 });
            }
            ReadOnly = (bool?)status["readOnly"] ?? false;
            BridgeEndpoint = (string)status["bridgeEndpoint"];
            var mode = (JObject)status["playbackMode"] ?? new JObject { ["active"] = false, ["playing"] = false };
            string encoded = mode.ToString(Formatting.None);
            if (encoded != previousMode)
            {
                Queue(new JObject { ["type"] = "replay", ["mode"] = mode });
                previousMode = encoded;
            }
        }

        private void QueueBundles(JArray bundles)
        {
            foreach (JObject bundle in bundles)
                foreach (JObject message in (JArray)bundle["messages"])
                    Queue(new JObject { ["type"] = "osc", ["address"] = message["address"], ["args"] = message["args"] });
        }

        private void Queue(JObject message)
        {
            if (incoming.Count >= 8192) throw new InvalidOperationException("Native projection queue exceeded; restart required");
            incoming.Enqueue(message.ToString(Formatting.None));
        }

        private delegate int Reader(IntPtr application, [Out] byte[] buffer, UIntPtr capacity, out UIntPtr required);
        private string Read(Reader reader)
        {
            int status = reader(handle, null, UIntPtr.Zero, out UIntPtr required);
            if (status == Empty) return "[]";
            if (status == Ok && required == UIntPtr.Zero) return "";
            if (status != BufferTooSmall) Check(status);
            ulong count = required.ToUInt64();
            if (count > MaximumBytes) throw new InvalidOperationException("Native response exceeds 64 MiB");
            for (int attempt = 0; attempt < 8; attempt++)
            {
                var bytes = new byte[(int)count];
                status = reader(handle, bytes, Size(bytes.Length), out UIntPtr copied);
                ulong nextCount = copied.ToUInt64();
                if (nextCount > MaximumBytes) throw new InvalidOperationException("Native response exceeds 64 MiB");
                if (status == BufferTooSmall)
                {
                    // Host metadata can change while a bridge worker completes
                    // a seek between the query and copy. No partial bytes escape.
                    count = Math.Min((ulong)MaximumBytes, Math.Max(nextCount, Math.Max(256UL, count * 2)));
                    continue;
                }
                Check(status);
                if (nextCount > (ulong)bytes.Length) throw new InvalidOperationException("Native response exceeded its caller-owned buffer");
                return Encoding.UTF8.GetString(bytes, 0, (int)nextCount);
            }
            throw new InvalidOperationException("Native response kept changing during inspection");
        }

        private void Check(int status)
        {
            if (status == Ok) return;
            string diagnostic = "";
            int queried = Native.LastError(handle, null, UIntPtr.Zero, out UIntPtr required);
            if ((queried == BufferTooSmall || queried == Ok) && required.ToUInt64() <= MaximumBytes)
            {
                var bytes = new byte[(int)required.ToUInt64()];
                if (Native.LastError(handle, bytes, Size(bytes.Length), out UIntPtr copied) == Ok)
                    diagnostic = Encoding.UTF8.GetString(bytes, 0, checked((int)copied.ToUInt64()));
            }
            throw new NativeFailure(status, diagnostic);
        }

        private sealed class NativeFailure : InvalidOperationException
        {
            internal int Code { get; }
            internal NativeFailure(int code, string diagnostic)
                : base("Native operation failed (" + code + "): " + diagnostic) { Code = code; }
        }

        private static UIntPtr Size(int value) => new UIntPtr(checked((uint)value));
        private void EnsureOwner()
        {
            if (Thread.CurrentThread.ManagedThreadId != ownerThread)
                throw new InvalidOperationException("Native Arena calls must stay on the creating Unity thread");
        }
        private void EnsureUsable()
        {
            if (handle == IntPtr.Zero) throw new ObjectDisposedException(nameof(ArenaNativeConnection));
            if (faulted) throw new InvalidOperationException(Status);
        }

        public void Dispose()
        {
            EnsureOwner();
            if (handle == IntPtr.Zero) return;
            IntPtr owned = handle;
            int status = Native.Destroy(owned);
            if (status != Ok && status != -6) Check(status);
            handle = IntPtr.Zero;
            attached = false;
            incoming.Clear();
            live.Remove(this);
            Status = "Embedded Runtime disposed";
        }

        private static void DisposeAll()
        {
            foreach (var connection in live.ToArray())
            {
                try { connection.Dispose(); }
                catch (Exception error) { Debug.LogException(error); }
            }
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        private static void ResetPlaySession() => DisposeAll();

#if UNITY_EDITOR
        [UnityEditor.InitializeOnLoadMethod]
        private static void InstallEditorCleanup()
        {
            UnityEditor.AssemblyReloadEvents.beforeAssemblyReload -= DisposeAll;
            UnityEditor.AssemblyReloadEvents.beforeAssemblyReload += DisposeAll;
            UnityEditor.EditorApplication.playModeStateChanged -= PlayModeChanged;
            UnityEditor.EditorApplication.playModeStateChanged += PlayModeChanged;
        }
        private static void PlayModeChanged(UnityEditor.PlayModeStateChange state)
        {
            if (state == UnityEditor.PlayModeStateChange.ExitingPlayMode) DisposeAll();
        }
#endif

        private static class Native
        {
            private const string Library = "kitu_demo_game_native";
            [DllImport(Library, EntryPoint = "kitu_application_abi_version", CallingConvention = CallingConvention.Cdecl)]
            internal static extern uint AbiVersion();
            [DllImport(Library, EntryPoint = "kitu_application_create", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Create(uint abi, [In] byte[] config, UIntPtr length, out IntPtr application,
                [Out] byte[] error, UIntPtr capacity, out UIntPtr required);
            [DllImport(Library, EntryPoint = "kitu_application_destroy", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Destroy(IntPtr application);
            [DllImport(Library, EntryPoint = "kitu_application_submit_json", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Submit(IntPtr application, [In] byte[] bytes, UIntPtr length, out ulong sequence);
            [DllImport(Library, EntryPoint = "kitu_application_tick", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Tick(IntPtr application);
            [DllImport(Library, EntryPoint = "kitu_application_read_output", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int ReadOutput(IntPtr application, [Out] byte[] buffer, UIntPtr capacity, out UIntPtr required);
            [DllImport(Library, EntryPoint = "kitu_application_inspect_json", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int Inspect(IntPtr application, [Out] byte[] buffer, UIntPtr capacity, out UIntPtr required);
            [DllImport(Library, EntryPoint = "kitu_application_inspect_host_json", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int InspectHost(IntPtr application, [Out] byte[] buffer, UIntPtr capacity, out UIntPtr required);
            [DllImport(Library, EntryPoint = "kitu_application_last_error", CallingConvention = CallingConvention.Cdecl)]
            internal static extern int LastError(IntPtr application, [Out] byte[] buffer, UIntPtr capacity, out UIntPtr required);
        }
    }
}
