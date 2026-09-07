using System.Collections;
using UnityEngine;

namespace UnityOnlyArena
{
    public sealed class ArenaAssetRetirement : MonoBehaviour
    {
        private ArenaAddressableAssets assets;
        public void Release(ArenaAddressableAssets value)
        {
            assets = value;
            DontDestroyOnLoad(gameObject);
            StartCoroutine(Retire());
        }
        private IEnumerator Retire()
        {
            yield return null;
            assets?.Dispose(); assets = null;
            Destroy(gameObject);
        }
        private void OnDestroy() { assets?.Dispose(); assets = null; }
    }
}
