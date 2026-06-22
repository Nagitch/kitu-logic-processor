using UnityEngine;
using UnityEngine.InputSystem;

namespace Kitu.Runtime
{
    [RequireComponent(typeof(KituNetworkRuntimeClient))]
    public sealed class KituNetworkPlayerController : MonoBehaviour
    {
        [SerializeField] private string entityId = "player:local";
        [SerializeField] private Transform playerView;
        [SerializeField] private float moveUnitsPerSecond = 3f;

        private KituNetworkRuntimeClient _client;

        private void Awake()
        {
            _client = GetComponent<KituNetworkRuntimeClient>();
        }

        private void OnEnable()
        {
            if (_client != null)
            {
                _client.RenderTransformReceived += OnRenderTransformReceived;
            }
        }

        private void OnDisable()
        {
            if (_client != null)
            {
                _client.RenderTransformReceived -= OnRenderTransformReceived;
            }
        }

        private void Update()
        {
            if (_client == null || !_client.IsConnected)
            {
                return;
            }

            var axis = ReadMovementInput();
            if (axis.sqrMagnitude <= 0.0001f)
            {
                return;
            }

            axis = Vector2.ClampMagnitude(axis, 1f);
            _client.SubmitMoveInput(entityId, axis * moveUnitsPerSecond * Time.deltaTime);
        }

        private static Vector2 ReadMovementInput()
        {
            var axis = Vector2.zero;

            var keyboard = Keyboard.current;
            if (keyboard != null)
            {
                if (keyboard.aKey.isPressed || keyboard.leftArrowKey.isPressed)
                {
                    axis.x -= 1f;
                }

                if (keyboard.dKey.isPressed || keyboard.rightArrowKey.isPressed)
                {
                    axis.x += 1f;
                }

                if (keyboard.sKey.isPressed || keyboard.downArrowKey.isPressed)
                {
                    axis.y -= 1f;
                }

                if (keyboard.wKey.isPressed || keyboard.upArrowKey.isPressed)
                {
                    axis.y += 1f;
                }
            }

            var gamepad = Gamepad.current;
            if (gamepad != null)
            {
                var stick = gamepad.leftStick.ReadValue();
                if (stick.sqrMagnitude > axis.sqrMagnitude)
                {
                    axis = stick;
                }
            }

            return axis;
        }

        private void OnRenderTransformReceived(KituRenderTransformEvent renderEvent)
        {
            if (renderEvent.EntityId != entityId || playerView == null)
            {
                return;
            }

            playerView.position = new Vector3(renderEvent.X, 0f, renderEvent.Y);
        }
    }
}
