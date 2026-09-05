using System;
using System.Collections.Concurrent;
using System.IO;
using System.Net.WebSockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace UnityOnlyArena
{
    // Transport only: no Unity objects, gameplay clocks or retry of consumed inputs.
    public sealed class ArenaConnection : IDisposable
    {
        private readonly ClientWebSocket socket = new ClientWebSocket();
        private readonly CancellationTokenSource cancellation = new CancellationTokenSource();
        private readonly ConcurrentQueue<string> outgoing = new ConcurrentQueue<string>();
        private readonly ConcurrentQueue<string> incoming = new ConcurrentQueue<string>();
        private readonly SemaphoreSlim ready = new SemaphoreSlim(0);
        private volatile bool connected;
        private volatile string status = "Connecting";
        public bool Connected => connected;
        public string Status => status;

        public ArenaConnection(string url) { _ = Run(url); }
        public bool TryReceive(out string message) => incoming.TryDequeue(out message);

        public bool Send(string message)
        {
            if (!connected) return false;
            if (outgoing.Count >= 256) { status = "Send queue exceeded; reconnect required"; Dispose(); return false; }
            outgoing.Enqueue(message);
            ready.Release();
            return true;
        }

        private async Task Run(string url)
        {
            Task writer = null;
            try
            {
                await socket.ConnectAsync(new Uri(url), cancellation.Token).ConfigureAwait(false);
                connected = true;
                status = "Connected";
                writer = WriteLoop();
                var buffer = new byte[16384];
                using (var message = new MemoryStream())
                {
                    while (!cancellation.IsCancellationRequested)
                    {
                        var part = await socket.ReceiveAsync(new ArraySegment<byte>(buffer), cancellation.Token).ConfigureAwait(false);
                        if (part.MessageType == WebSocketMessageType.Close) break;
                        if (part.MessageType != WebSocketMessageType.Text) throw new InvalidDataException("Arena requires JSON text frames");
                        message.Write(buffer, 0, part.Count);
                        if (message.Length > 8 * 1024 * 1024) throw new InvalidDataException("Arena projection too large");
                        if (!part.EndOfMessage) continue;
                        if (incoming.Count >= 1024) throw new InvalidDataException("Receive queue exceeded; reconnect required");
                        incoming.Enqueue(Encoding.UTF8.GetString(message.ToArray()));
                        message.SetLength(0);
                    }
                }
            }
            catch (Exception error) when (error is WebSocketException || error is OperationCanceledException || error is InvalidDataException || error is UriFormatException)
            { status = error is OperationCanceledException ? "Disconnected" : error.Message; }
            finally
            {
                connected = false;
                if (status == "Connected") status = "Disconnected";
                cancellation.Cancel();
                socket.Abort();
                if (writer != null) { try { await writer.ConfigureAwait(false); } catch (Exception) { /* Receive loop already reports connection loss. */ } }
                socket.Dispose();
                // These remain usable by a late Send/Dispose on the Unity thread.
            }
        }

        private async Task WriteLoop()
        {
            try
            {
                while (!cancellation.IsCancellationRequested)
                {
                    await ready.WaitAsync(cancellation.Token).ConfigureAwait(false);
                    if (!outgoing.TryDequeue(out string message)) continue;
                    byte[] bytes = Encoding.UTF8.GetBytes(message);
                    await socket.SendAsync(new ArraySegment<byte>(bytes), WebSocketMessageType.Text, true, cancellation.Token).ConfigureAwait(false);
                }
            }
            finally { cancellation.Cancel(); socket.Abort(); }
        }

        public void Dispose()
        {
            connected = false;
            cancellation.Cancel();
            try { socket.Abort(); } catch (ObjectDisposedException) { }
        }
    }
}
