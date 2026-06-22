using System;
using System.Collections.Concurrent;
using System.Net.WebSockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace Kitu.Runtime
{
    public sealed class KituNetworkRuntimeClient : MonoBehaviour
    {
        [SerializeField] private string runtimeWebSocketUrl = "ws://127.0.0.1:8787/ws/runtime";

        private readonly ConcurrentQueue<string> _incoming = new ConcurrentQueue<string>();
        private readonly SemaphoreSlim _sendLock = new SemaphoreSlim(1, 1);
        private ClientWebSocket _socket;
        private CancellationTokenSource _cts;

        public event Action<KituRenderTransformEvent> RenderTransformReceived;
        public event Action<KituWorldSnapshotEvent> WorldSnapshotReceived;

        public bool IsConnected => _socket != null && _socket.State == WebSocketState.Open;

        private void OnEnable()
        {
            _cts = new CancellationTokenSource();
            _ = ConnectAsync(_cts.Token);
        }

        private void Update()
        {
            while (_incoming.TryDequeue(out var json))
            {
                if (KituOscJson.TryParseRenderTransform(json, out var renderEvent))
                {
                    RenderTransformReceived?.Invoke(renderEvent);
                }

                if (KituOscJson.TryParseWorldSnapshot(json, out var snapshotEvent))
                {
                    WorldSnapshotReceived?.Invoke(snapshotEvent);
                }
            }
        }

        private async void OnDisable()
        {
            _cts?.Cancel();

            if (_socket != null)
            {
                try
                {
                    if (_socket.State == WebSocketState.Open)
                    {
                        await _socket.CloseAsync(
                            WebSocketCloseStatus.NormalClosure,
                            "Unity client disabled",
                            CancellationToken.None);
                    }
                }
                catch (WebSocketException)
                {
                }
                finally
                {
                    _socket.Dispose();
                    _socket = null;
                }
            }

            _cts?.Dispose();
            _cts = null;
        }

        public void SubmitMoveInput(string entityId, Vector2 movementDelta)
        {
            if (!IsConnected)
            {
                return;
            }

            var json = KituOscJson.BuildMoveInput(entityId, movementDelta.x, movementDelta.y);
            _ = SendTextAsync(json, _cts?.Token ?? CancellationToken.None);
        }

        private async Task ConnectAsync(CancellationToken token)
        {
            _socket = new ClientWebSocket();

            try
            {
                await _socket.ConnectAsync(new Uri(runtimeWebSocketUrl), token);
                await ReceiveLoopAsync(token);
            }
            catch (OperationCanceledException)
            {
            }
            catch (WebSocketException exception)
            {
                Debug.LogWarning($"Kitu runtime WebSocket connection failed: {exception.Message}");
            }
            catch (Exception exception)
            {
                Debug.LogException(exception);
            }
        }

        private async Task SendTextAsync(string json, CancellationToken token)
        {
            var hasLock = false;
            try
            {
                await _sendLock.WaitAsync(token);
                hasLock = true;

                if (!IsConnected)
                {
                    return;
                }

                var bytes = Encoding.UTF8.GetBytes(json);
                await _socket.SendAsync(
                    new ArraySegment<byte>(bytes),
                    WebSocketMessageType.Text,
                    true,
                    token);
            }
            catch (OperationCanceledException)
            {
            }
            catch (WebSocketException exception)
            {
                Debug.LogWarning($"Kitu runtime WebSocket send failed: {exception.Message}");
            }
            finally
            {
                if (hasLock)
                {
                    _sendLock.Release();
                }
            }
        }

        private async Task ReceiveLoopAsync(CancellationToken token)
        {
            var buffer = new byte[4096];
            var builder = new StringBuilder();

            while (!token.IsCancellationRequested && _socket.State == WebSocketState.Open)
            {
                var result = await _socket.ReceiveAsync(new ArraySegment<byte>(buffer), token);
                if (result.MessageType == WebSocketMessageType.Close)
                {
                    break;
                }

                builder.Append(Encoding.UTF8.GetString(buffer, 0, result.Count));
                if (!result.EndOfMessage)
                {
                    continue;
                }

                _incoming.Enqueue(builder.ToString());
                builder.Clear();
            }
        }
    }
}
