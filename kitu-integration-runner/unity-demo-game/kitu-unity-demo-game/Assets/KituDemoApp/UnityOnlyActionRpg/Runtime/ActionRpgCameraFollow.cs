using UnityEngine;

namespace UnityOnlyActionRpg
{
    public sealed class ActionRpgCameraFollow : MonoBehaviour
    {
        [SerializeField] private Transform target;
        [SerializeField] private Vector3 offset = new(0f, 9f, -12f);
        [SerializeField, Min(0f)] private float followSharpness = 8f;

        private void Start()
        {
            if (target == null)
            {
                target = FindFirstObjectByType<ActionRpgPlayerController>()?.transform;
            }
        }

        private void LateUpdate()
        {
            if (target == null)
            {
                return;
            }

            float blend = 1f - Mathf.Exp(-followSharpness * Time.deltaTime);
            transform.position = Vector3.Lerp(transform.position, target.position + offset, blend);
            transform.rotation = Quaternion.LookRotation(target.position + Vector3.up - transform.position, Vector3.up);
        }
    }
}
