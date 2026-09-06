using System;
using System.Collections;
using System.Collections.Generic;
using Newtonsoft.Json.Linq;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.ResourceManagement.AsyncOperations;
using UnityEngine.ResourceManagement.ResourceLocations;

namespace UnityOnlyArena
{
    // One set of real Addressables leases per view. Clones use the retained
    // prefabs; gameplay never waits for an individual projectile to load.
    public sealed class ArenaAddressableAssets : IDisposable
    {
        private readonly List<AsyncOperationHandle> handles = new List<AsyncOperationHandle>();
        private readonly Dictionary<string, GameObject> prefabs = new Dictionary<string, GameObject>();
        private bool disposed;
        public static int LiveHandleCount { get; private set; }
        public bool Ready { get; private set; }
        public string Error { get; private set; }
        public Material BaseMaterial { get; private set; }
        public JObject Report { get; private set; }

        public IEnumerator Load(ArenaContentPackage package)
        {
            if (disposed || handles.Count != 0) throw new InvalidOperationException("Arena asset owner cannot load twice");
            Report = new JObject { ["package"] = package.Report.DeepClone(), ["loaded"] = new JArray() };
            foreach (var entry in package.Keys)
            {
                if (disposed) yield break;
                AsyncOperationHandle handle = default;
                try
                {
                    handle = entry.Key == "baseMaterial"
                        ? (AsyncOperationHandle)Addressables.LoadAssetAsync<Material>(entry.Value)
                        : Addressables.LoadAssetAsync<GameObject>(entry.Value);
                    handles.Add(handle); LiveHandleCount++;
                }
                catch (Exception error) { Error = error.Message; Dispose(); yield break; }
                yield return handle;
                if (disposed) yield break;
                try
                {
                    if (handle.Status != AsyncOperationStatus.Succeeded || handle.Result == null)
                        throw new InvalidOperationException("Cannot load Arena Addressable " + entry.Value + ": " + handle.OperationException?.Message);
                    if (entry.Key == "baseMaterial")
                    {
                        BaseMaterial = handle.Result as Material;
                        if (BaseMaterial == null || BaseMaterial.shader == null || !BaseMaterial.shader.isSupported)
                            throw new InvalidOperationException("Arena base material has no supported shader");
                    }
                    else
                    {
                        var prefab = handle.Result as GameObject;
                        if (prefab == null || prefab.GetComponent<MeshFilter>()?.sharedMesh == null || prefab.GetComponent<Renderer>() == null ||
                            prefab.GetComponentsInChildren<Collider>(true).Length != 0 || prefab.GetComponentsInChildren<MonoBehaviour>(true).Length != 0)
                            throw new InvalidOperationException("Arena Addressable is not a visual primitive prefab: " + entry.Value);
                        prefabs.Add(entry.Key, prefab);
                    }
                    var locations = new JArray();
                    foreach (var locator in Addressables.ResourceLocators)
                        if (locator.Locate(entry.Value, entry.Key == "baseMaterial" ? typeof(Material) : typeof(GameObject), out var found))
                            foreach (var location in found)
                                locations.Add(LocationReport(location, 0));
                    if (locations.Count == 0) throw new InvalidOperationException("Arena Addressable has no resource location");
                    ((JArray)Report["loaded"]).Add(new JObject { ["role"] = entry.Key, ["key"] = entry.Value,
                        ["type"] = handle.Result.GetType().Name, ["locations"] = locations });
                }
                catch (Exception error) { Error = error.Message; Dispose(); yield break; }
            }
            Ready = !disposed;
        }

        private static JObject LocationReport(IResourceLocation location, int depth)
        {
            if (depth > 8 || (location.Dependencies?.Count ?? 0) > 32) throw new InvalidOperationException("Arena resource dependency limit");
            var dependencies = new JArray();
            if (location.Dependencies != null)
                foreach (var dependency in location.Dependencies) dependencies.Add(LocationReport(dependency, depth + 1));
            return new JObject { ["key"] = location.PrimaryKey, ["internalId"] = location.InternalId,
                ["resolvedInternalId"] = Addressables.ResourceManager.TransformInternalId(location),
                ["provider"] = location.ProviderId, ["dependencies"] = dependencies };
        }

        public GameObject Instantiate(PrimitiveType type)
        {
            if (!Ready) throw new InvalidOperationException("Arena Addressables are not ready");
            string role = type == PrimitiveType.Cube ? "cube" : type == PrimitiveType.Capsule ? "capsule" : type == PrimitiveType.Sphere ? "sphere"
                : throw new ArgumentException("Unsupported Arena visual primitive");
            return UnityEngine.Object.Instantiate(prefabs[role]);
        }

        public void Dispose()
        {
            if (disposed) return;
            disposed = true; Ready = false;
            prefabs.Clear(); BaseMaterial = null;
            foreach (var handle in handles)
            {
                if (handle.IsValid()) Addressables.Release(handle);
                LiveHandleCount--;
            }
            handles.Clear();
        }

        // Destroy() finishes at frame end. Keep the leases alive until the
        // retired view's clones and derived materials have been destroyed,
        // including when its original owner has already been disabled.
        public void ReleaseAfterViewDestroyed()
        {
            if (disposed) return;
            var retirement = new GameObject("Arena asset retirement").AddComponent<ArenaAssetRetirement>();
            retirement.Release(this);
        }
    }
}
