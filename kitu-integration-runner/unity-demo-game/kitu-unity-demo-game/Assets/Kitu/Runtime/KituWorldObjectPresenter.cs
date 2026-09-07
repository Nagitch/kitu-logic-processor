using System.Collections.Generic;
using UnityEngine;

namespace Kitu.Runtime
{
    [RequireComponent(typeof(KituNetworkRuntimeClient))]
    public sealed class KituWorldObjectPresenter : MonoBehaviour
    {
        [SerializeField] private Transform worldRoot;

        private readonly Dictionary<string, GameObject> _objects = new Dictionary<string, GameObject>();
        private KituNetworkRuntimeClient _client;

        private void Awake()
        {
            _client = GetComponent<KituNetworkRuntimeClient>();
            if (worldRoot == null)
            {
                var root = new GameObject("Kitu World Objects");
                worldRoot = root.transform;
            }
        }

        private void OnEnable()
        {
            if (_client != null)
            {
                _client.WorldSnapshotReceived += ApplySnapshot;
            }
        }

        private void OnDisable()
        {
            if (_client != null)
            {
                _client.WorldSnapshotReceived -= ApplySnapshot;
            }
        }

        private void ApplySnapshot(KituWorldSnapshotEvent snapshot)
        {
            var liveIds = new HashSet<string>();

            foreach (var worldObject in snapshot.Objects)
            {
                liveIds.Add(worldObject.Id);
                var view = GetOrCreateView(worldObject);
                view.transform.position = new Vector3(worldObject.X, worldObject.Y, worldObject.Z);
                ApplyColor(view, worldObject.Color);
            }

            var staleIds = new List<string>();
            foreach (var entry in _objects)
            {
                if (!liveIds.Contains(entry.Key))
                {
                    staleIds.Add(entry.Key);
                }
            }

            foreach (var id in staleIds)
            {
                Destroy(_objects[id]);
                _objects.Remove(id);
            }
        }

        private GameObject GetOrCreateView(KituWorldObjectState worldObject)
        {
            if (_objects.TryGetValue(worldObject.Id, out var existing))
            {
                return existing;
            }

            var primitive = PrimitiveForKind(worldObject.Kind);
            var view = GameObject.CreatePrimitive(primitive);
            view.name = $"Kitu {worldObject.Kind} {worldObject.Id}";
            view.transform.SetParent(worldRoot, true);
            view.transform.localScale = ScaleForKind(worldObject.Kind);
            _objects.Add(worldObject.Id, view);
            return view;
        }

        private static PrimitiveType PrimitiveForKind(string kind)
        {
            switch (kind)
            {
                case "enemy":
                    return PrimitiveType.Capsule;
                case "treasure":
                    return PrimitiveType.Sphere;
                case "trigger":
                    return PrimitiveType.Cylinder;
                default:
                    return PrimitiveType.Cube;
            }
        }

        private static Vector3 ScaleForKind(string kind)
        {
            switch (kind)
            {
                case "trigger":
                    return new Vector3(1.4f, 0.2f, 1.4f);
                case "treasure":
                    return Vector3.one * 0.7f;
                default:
                    return Vector3.one;
            }
        }

        private static void ApplyColor(GameObject view, string htmlColor)
        {
            if (string.IsNullOrEmpty(htmlColor) || !ColorUtility.TryParseHtmlString(htmlColor, out var color))
            {
                color = Color.white;
            }

            var renderer = view.GetComponent<Renderer>();
            if (renderer == null)
            {
                return;
            }

            if (renderer.material != null)
            {
                renderer.material.color = color;
            }
        }
    }
}
