using System;
using System.Collections.Concurrent;
using System.IO;
using System.Net.WebSockets;
using System.Threading;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;

namespace UnityOnlyArena
{
    // Owns transport admission/delivery only. Reconnect is explicit and never resends consumed input.
    public sealed class ArenaConnection : IArenaConnection
    {
        private const int MaximumQueuedFrames = 32, MaximumQueuedBytes = 16 * 1024 * 1024;
        private readonly ClientWebSocket socket = new ClientWebSocket();
        private readonly CancellationTokenSource cancellation = new CancellationTokenSource();
        private readonly ConcurrentQueue<byte[]> outgoing = new ConcurrentQueue<byte[]>();
        private readonly ConcurrentQueue<Received> incoming = new ConcurrentQueue<Received>();
        private readonly SemaphoreSlim ready = new SemaphoreSlim(0);
        private readonly ArenaWireCodec inputCodec, outputCodec;
        private readonly WebSocketMessageType frameType;
        private readonly string clientId, expectedSessionId, protocol;
        private long incomingBytes;
        private volatile bool connected, closeRequested;
        private volatile string status = "Connecting";
        private ulong deliverySequence;
        private bool receivedHello, receivedSnapshot;
        public bool Connected => connected;
        public string Status => status;
        public string SessionId { get; private set; }
        public JObject Execution { get; private set; }
        public ArenaWireEncoding Encoding { get; }
        public Task Completion { get; }
        private sealed class Received
        {
            internal JObject Frame;
            internal int Bytes;
        }

        public ArenaConnection(string url, string clientId, ArenaWireEncoding encoding, string expectedSessionId = null, Task previousConnectionClosed = null)
        {
            this.clientId = clientId; this.expectedSessionId = expectedSessionId; Encoding = encoding;
            inputCodec = new ArenaWireCodec(encoding); outputCodec = new ArenaWireCodec(encoding, true);
            protocol = encoding == ArenaWireEncoding.Json ? "kitu-arena-json-v1" : "kitu-arena-msgpack-v1";
            frameType = encoding == ArenaWireEncoding.Json ? WebSocketMessageType.Text : WebSocketMessageType.Binary;
            socket.Options.AddSubProtocol(protocol);
            Completion = Run(url, previousConnectionClosed);
        }
        public bool TryReceive(out JObject message)
        {
            if (!incoming.TryDequeue(out var received)) { message = null; return false; }
            Interlocked.Add(ref incomingBytes, -received.Bytes); message = received.Frame; return true;
        }
        public void Pump(double elapsedSeconds) { }
        public void Disconnect()
        {
            connected = false; closeRequested = true;
            while (outgoing.TryDequeue(out _)) { }
            while (incoming.TryDequeue(out var discarded)) Interlocked.Add(ref incomingBytes, -discarded.Bytes);
            cancellation.CancelAfter(TimeSpan.FromSeconds(3)); ready.Release();
        }
        public bool Send(JObject input)
        {
            if (!connected) return false;
            if ((string)input["metadata"]?["source"] != clientId) throw new InvalidDataException("Input producer differs from Arena handshake");
            byte[] bytes = inputCodec.EncodeClient(new JObject { ["type"] = "input", ["payload"] = input });
            if (outgoing.Count >= 256) { status = "Send queue exceeded; reconnect required"; Dispose(); return false; }
            outgoing.Enqueue(bytes); ready.Release(); return true;
        }
        private async Task Run(string url, Task previousConnectionClosed)
        {
            Task writer = null;
            try
            {
                if (previousConnectionClosed != null) await previousConnectionClosed.ConfigureAwait(false);
                cancellation.Token.ThrowIfCancellationRequested();
                using (var handshake = CancellationTokenSource.CreateLinkedTokenSource(cancellation.Token))
                {
                    handshake.CancelAfter(TimeSpan.FromSeconds(5));
                    await socket.ConnectAsync(new Uri(url), handshake.Token).ConfigureAwait(false);
                    if (socket.SubProtocol != protocol) throw new InvalidDataException("Server did not negotiate the Arena wire protocol");
                    byte[] hello = inputCodec.EncodeClient(ArenaWireCodec.Hello(clientId, expectedSessionId));
                    await socket.SendAsync(new ArraySegment<byte>(hello), frameType, true, handshake.Token).ConfigureAwait(false);
                    writer = WriteLoop();
                    var buffer = new byte[16384];
                    using (var message = new MemoryStream())
                    {
                        while (!cancellation.IsCancellationRequested)
                        {
                            var token = receivedSnapshot ? cancellation.Token : handshake.Token;
                            var part = await socket.ReceiveAsync(new ArraySegment<byte>(buffer), token).ConfigureAwait(false);
                            if (part.MessageType == WebSocketMessageType.Close)
                            {
                                status = "Disconnected: " + (part.CloseStatusDescription ?? "server closed Arena connection"); break;
                            }
                            if (part.MessageType != frameType) throw new InvalidDataException("Arena frame encoding differs from negotiated protocol");
                            if (part.Count > ArenaWireCodec.OutputBytes - message.Length) throw new InvalidDataException("Arena projection exceeds output limit");
                            message.Write(buffer, 0, part.Count);
                            if (!part.EndOfMessage) continue;
                            byte[] bytes = message.ToArray(); message.SetLength(0);
                            JObject frame = outputCodec.DecodeServer(bytes);
                            Admit(frame);
                            if (incoming.Count >= MaximumQueuedFrames || Interlocked.Read(ref incomingBytes) + bytes.Length > MaximumQueuedBytes)
                                throw new InvalidDataException("Receive queue exceeded; resynchronization by reconnect required");
                            Interlocked.Add(ref incomingBytes, bytes.Length); incoming.Enqueue(new Received { Frame = frame, Bytes = bytes.Length });
                            if (receivedSnapshot && !closeRequested) { connected = true; status = "Connected (" + Encoding + ")"; }
                            var payload = frame["frame"]["payload"];
                            if ((string)frame["frame"]["type"] == "error" && (bool)payload["fatal"])
                                throw new InvalidDataException((string)payload["message"]);
                        }
                    }
                }
            }
            catch (Exception error)
            {
                status = error is OperationCanceledException ? "Disconnected or Arena handshake timed out" : error.Message;
            }
            finally
            {
                connected = false;
                cancellation.Cancel(); socket.Abort();
                if (writer != null) { try { await writer.ConfigureAwait(false); } catch (Exception) { /* Receive loop reports connection loss. */ } }
                socket.Dispose();
            }
        }
        private void Admit(JObject frame)
        {
            ulong sequence = ArenaWireCodec.Unsigned(frame["deliverySequence"]);
            if (sequence != deliverySequence) throw new InvalidDataException("Arena delivery gap; reconnect to resynchronize");
            if (deliverySequence == ulong.MaxValue) throw new InvalidDataException("Arena delivery sequence exhausted; reconnect required");
            deliverySequence++;
            string type = (string)frame["frame"]["type"]; var payload = frame["frame"]["payload"];
            if (!receivedHello)
            {
                if (type == "error" && (bool)payload["fatal"]) return;
                if (type != "hello") throw new InvalidDataException("Arena server hello must precede game data");
                ArenaWireCodec.CheckCompatibility(payload["compatibility"]);
                if ((string)payload["role"] != "controller") throw new InvalidDataException("Arena server did not grant requested controller role");
                if ((uint)payload["limits"]["maxInputBytes"] != ArenaWireCodec.InputBytes || (uint)payload["limits"]["maxOutputBytes"] != ArenaWireCodec.OutputBytes)
                    throw new InvalidDataException("Incompatible Arena wire limits");
                SessionId = (string)payload["sessionId"];
                if (string.IsNullOrEmpty(SessionId) || (expectedSessionId != null && expectedSessionId != SessionId))
                    throw new InvalidDataException("Arena session changed; create an explicit new attachment");
                Execution = (JObject)payload["execution"]; receivedHello = true; return;
            }
            if (type == "hello") throw new InvalidDataException("Duplicate Arena server hello");
            if (!receivedSnapshot)
            {
                if (type == "error") return;
                if (type != "snapshot" || (string)payload["reason"] != "initial") throw new InvalidDataException("Arena initial snapshot must precede updates");
                receivedSnapshot = true;
            }
            else if (type == "snapshot" && (string)payload["reason"] == "initial") throw new InvalidDataException("Unexpected second initial snapshot");
        }
        private async Task WriteLoop()
        {
            try
            {
                while (!cancellation.IsCancellationRequested)
                {
                    await ready.WaitAsync(cancellation.Token).ConfigureAwait(false);
                    if (closeRequested)
                    {
                        await socket.CloseOutputAsync(WebSocketCloseStatus.NormalClosure, "Arena controller detached", cancellation.Token).ConfigureAwait(false);
                        return; // The receive owner waits for the peer's close acknowledgment before completing.
                    }
                    if (!outgoing.TryDequeue(out var bytes)) continue;
                    await socket.SendAsync(new ArraySegment<byte>(bytes), frameType, true, cancellation.Token).ConfigureAwait(false);
                }
            }
            catch { cancellation.Cancel(); socket.Abort(); throw; }
        }
        public void Dispose()
        {
            connected = false; cancellation.Cancel();
            try { socket.Abort(); } catch (ObjectDisposedException) { }
            while (incoming.TryDequeue(out var discarded)) Interlocked.Add(ref incomingBytes, -discarded.Bytes);
            while (outgoing.TryDequeue(out _)) { }
        }
    }
}
