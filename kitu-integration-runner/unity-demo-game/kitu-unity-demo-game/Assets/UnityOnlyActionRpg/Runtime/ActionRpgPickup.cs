using UnityEngine;

namespace UnityOnlyActionRpg
{
    public sealed class ActionRpgPickup : MonoBehaviour
    {
        private float baseY;
        private ActionRpgPlayerController player;
        private ActionRpgGameController game;
        private bool collected;

        private void Start()
        {
            baseY = transform.position.y;
            player = FindFirstObjectByType<ActionRpgPlayerController>();
            game = FindFirstObjectByType<ActionRpgGameController>();
        }

        private void Update()
        {
            transform.Rotate(Vector3.up, 100f * Time.deltaTime, Space.World);
            Vector3 position = transform.position;
            position.y = baseY + Mathf.Sin(Time.time * 4f) * 0.12f;
            transform.position = position;

            if (player != null && (player.transform.position - transform.position).sqrMagnitude <= 0.8f * 0.8f)
            {
                Collect(player);
            }
        }

        private void OnTriggerEnter(Collider other)
        {
            ActionRpgPlayerController enteringPlayer = other.GetComponentInParent<ActionRpgPlayerController>();
            if (enteringPlayer != null)
            {
                Collect(enteringPlayer);
            }
        }

        private void Collect(ActionRpgPlayerController collectingPlayer)
        {
            if (collected || collectingPlayer == null)
            {
                return;
            }

            collected = true;
            game ??= FindFirstObjectByType<ActionRpgGameController>();
            game?.CollectPotion();
            Destroy(gameObject);
        }
    }
}
