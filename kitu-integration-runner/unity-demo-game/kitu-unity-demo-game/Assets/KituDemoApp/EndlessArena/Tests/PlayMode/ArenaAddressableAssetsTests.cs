using System;
using System.Collections;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.TestTools;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaAddressableAssetsTests
    {
        private KituArenaClient client;
        private ArenaAddressableAssets loadingOwner;
        private string directory;
        private int assetBaseline, nativeBaseline;
        private bool previousIgnoreLogs;

        [UnitySetUp]
        public IEnumerator Setup()
        {
            previousIgnoreLogs = LogAssert.ignoreFailingMessages;
            // Earlier scene fixtures retire their Addressables at frame end.
            yield return null;
            yield return null;
            assetBaseline = ArenaAddressableAssets.LiveHandleCount;
            nativeBaseline = ArenaNativeConnection.LiveHandleCount;
            directory = Path.Combine(Path.GetTempPath(), "arena-assets-test-" + Guid.NewGuid().ToString("N"));
            string source = Path.Combine(Application.streamingAssetsPath, "KituArena");
            Assert.That(File.Exists(Path.Combine(source, "package.json")), Is.True, "Prepare the real Arena package before PlayMode tests");
            foreach (string file in Directory.GetFiles(source, "*", SearchOption.AllDirectories))
            {
                string relative = file.Substring(source.Length + 1);
                string target = Path.Combine(directory, relative);
                Directory.CreateDirectory(Path.GetDirectoryName(target));
                File.Copy(file, target);
            }
        }

        [UnityTearDown]
        public IEnumerator Cleanup()
        {
            if (client != null) { client.gameObject.SetActive(false); UnityEngine.Object.Destroy(client.gameObject); }
            loadingOwner?.Dispose(); loadingOwner = null;
            yield return null;
            yield return null;
            yield return null;
            LogAssert.ignoreFailingMessages = previousIgnoreLogs;
            if (Directory.Exists(directory)) Directory.Delete(directory, true);
            Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline), "test must release every native owner");
            Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline), "test must release every Addressables lease");
        }

        [UnityTest]
        public IEnumerator EarlyRepeatedConnectUsesFinalManualSettingsAndCreatesOneView()
        {
            RequireNative();
            CreateClient();
            client.NativeAutomaticTicks = true;
            client.Connect();
            client.NativeAutomaticTicks = false;
            client.NativeBridgeEnabled = false;
            client.Connect();
            Assert.That(client.Connected, Is.False);
            Assert.That(client.NativeConnection, Is.Null);
            Assert.That(client.Command("start"), Is.False);
            Assert.That(client.Frame(Vector2.one, Vector2.up), Is.False);
            yield return Ready();
            AssertOneView();
            Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline + 1));
            Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline + 4));
            Assert.That(client.NativeConnection.BridgeEndpoint, Is.Null);
            for (int frame = 0; frame < 8; frame++) yield return null;
            Assert.That(client.State.tick, Is.EqualTo(-1), "asset waiting and elapsed frames cannot advance a manual Runtime");
            client.NativeConnection.Step();
            yield return null;
            Assert.That(client.State.tick, Is.EqualTo(0));
        }

        [UnityTest]
        public IEnumerator DisconnectBeforeReadinessCancelsConnectButRetainsLoadedAssets()
        {
            RequireNative();
            CreateClient();
            client.Connect();
            client.Disconnect();
            yield return Until(() => client.AssetsReady, "assets after cancelled connection request");
            for (int frame = 0; frame < 3; frame++) yield return null;
            Assert.That(client.Connected, Is.False);
            Assert.That(client.NativeConnection, Is.Null);
            Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline));
            JObject report = client.AssetReport;
            client.Connect();
            yield return Ready();
            Assert.That(client.AssetReport, Is.SameAs(report));
            var native = client.NativeConnection;
            client.Disconnect();
            client.Connect();
            yield return Ready();
            Assert.That(client.NativeConnection, Is.SameAs(native));
            Assert.That(client.AssetReport, Is.SameAs(report));
            AssertOneView();
        }

        [UnityTest]
        public IEnumerator DestroyBeforePreparationAndDisposeDuringRealLoadNeverAdoptLateResults()
        {
            CreateClient();
            client.Connect();
            client.gameObject.SetActive(false);
            UnityEngine.Object.Destroy(client.gameObject);
            client = null;
            // Advance to the real Addressables operation's yield boundary, then
            // dispose before its result can be adopted (also covers warm loads).
            loadingOwner = new ArenaAddressableAssets();
            IEnumerator loading = loadingOwner.Load(ArenaContentPackage.Load(directory));
            Assert.That(loading.MoveNext(), Is.True);
            Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline + 1));
            loadingOwner.Dispose();
            // A released handle may already be invalid, so never yield or inspect it.
            yield return null;
            Assert.That(loading.MoveNext(), Is.False);
            Assert.That(loadingOwner.Ready, Is.False);
            Assert.That(loadingOwner.BaseMaterial, Is.Null);
            for (int frame = 0; frame < 3; frame++) yield return null;
            Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline));
            Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline));
        }

        [UnityTest]
        public IEnumerator DisableReenableRetiresClonesBeforeFreshSingleOwner()
        {
            RequireNative();
            CreateClient(); client.Connect();
            yield return Ready();
            for (int cycle = 0; cycle < 3; cycle++)
            {
                var owner = client.NativeConnection;
                string session = client.SessionId;
                var renderers = client.GetComponentsInChildren<Renderer>(true);
                Material[] materials = renderers.SelectMany(renderer => renderer.sharedMaterials).Distinct().ToArray();
                client.gameObject.SetActive(false);
                Assert.That(owner.IsDisposed, Is.True, "native ownership is released synchronously");
                yield return null;
                yield return null;
                Assert.That(renderers.All(renderer => renderer == null), Is.True);
                Assert.That(materials.All(material => material == null), Is.True, "derived materials retire before asset leases");
                Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline));
                client.gameObject.SetActive(true); client.Connect();
                yield return Ready();
                Assert.That(client.NativeConnection, Is.Not.SameAs(owner));
                Assert.That(client.SessionId, Is.Not.EqualTo(session));
                Assert.That(client.State.tick, Is.EqualTo(-1));
                AssertOneView();
                Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline + 1));
                Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline + 4));
            }
        }

        [UnityTest]
        public IEnumerator PackageChangedAtRealAssetResolutionIsRejectedBeforeNativeCreation()
        {
            RequireNative();
            string initialIdentity = ArenaContentPackage.Load(directory).Identity;
            string capturedIdentity = null, changedIdentity = null;
            Exception mutationFailure = null;
            int mutations = 0;
            var previousTransform = Addressables.InternalIdTransformFunc;
            LogAssert.ignoreFailingMessages = true;
            try
            {
                Addressables.InternalIdTransformFunc = location =>
                {
                    // The real loader publishes its captured package before
                    // asking Addressables to resolve any asset. Its location
                    // report also resolves IDs, so warm loads reach this same
                    // deterministic boundary without clearing global caches.
                    if (mutations == 0 && client?.AssetReport?["package"] != null)
                    {
                        capturedIdentity = (string)client.AssetReport["package"]["identity"];
                        mutations++;
                        try
                        {
                            // Both keys still resolve to valid GameObjects and
                            // every digest is recomputed. Only the package
                            // generation differs from the captured visual data.
                            MutateMapping(assets => {
                                JToken cubeKey = assets[1]["key"].DeepClone();
                                assets[1]["key"] = assets[3]["key"].DeepClone();
                                assets[3]["key"] = cubeKey;
                            });
                            changedIdentity = ArenaContentPackage.Load(directory).Identity;
                        }
                        catch (Exception error) { mutationFailure = error; }
                    }
                    return previousTransform == null ? location.InternalId : previousTransform(location);
                };
                CreateClient(); client.Connect();
                yield return Until(() => client.Connected || !string.IsNullOrEmpty(client.PreparationError),
                    "changed package rejection after actual Addressables resolution");
                Assert.That(mutations, Is.EqualTo(1), "the actual resolution callback must reach the captured-package boundary exactly once");
                Assert.That(mutationFailure, Is.Null, "the replacement package must be valid: " + mutationFailure);
                Assert.That(capturedIdentity, Is.EqualTo(initialIdentity));
                Assert.That(changedIdentity, Is.Not.Null.And.Not.EqualTo(initialIdentity));
                StringAssert.Contains("package hash mismatch", client.PreparationError);
                Assert.That(client.Connected, Is.False);
                Assert.That(client.NativeConnection, Is.Null);
                Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline));
                Assert.That(client.State.tick, Is.EqualTo(-1));
                Assert.That(client.AssetsReady, Is.False);
                Assert.That(client.GameCamera, Is.Null);
                yield return null;
                yield return null;
                Assert.That(client.transform.Find("Arena presentation"), Is.Null, "the prepared view must be retired after native refusal");
                Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline));
                Assert.That(Directory.Exists(Path.Combine(directory, "storage")), Is.False,
                    "a rejected initializer must not create authoring storage");
            }
            finally
            {
                Addressables.InternalIdTransformFunc = previousTransform;
                LogAssert.ignoreFailingMessages = previousIgnoreLogs;
            }
        }

        [UnityTest]
        public IEnumerator RehashedMissingKeyFailsBeforeNativeCreationWithoutProceduralFallback()
        {
            MutateMapping(assets => assets[0]["key"] = "arena/missing/" + Guid.NewGuid().ToString("N"));
            yield return RejectedAssets();
        }

        [UnityTest]
        public IEnumerator RehashedWrongLoadedTypeFailsBeforeNativeCreationWithoutProceduralFallback()
        {
            MutateMapping(assets => {
                JToken materialKey = assets[0]["key"].DeepClone();
                assets[0]["key"] = assets[1]["key"].DeepClone();
                assets[1]["key"] = materialKey;
            });
            yield return RejectedAssets();
        }

        private IEnumerator RejectedAssets()
        {
            // Addressables logs its diagnostic as well as returning failed status.
            // Assert that specific product failure below and restore logging in teardown.
            LogAssert.ignoreFailingMessages = true;
            CreateClient(); client.Connect();
            yield return Until(() => !string.IsNullOrEmpty(client.PreparationError), "explicit asset failure");
            StringAssert.Contains("Arena Addressable", client.PreparationError);
            Assert.That(client.Connected, Is.False);
            Assert.That(client.AssetsReady, Is.False);
            Assert.That(client.NativeConnection, Is.Null);
            Assert.That(client.GameCamera, Is.Null);
            Assert.That(client.transform.Find("Arena presentation"), Is.Null);
            Assert.That(client.State.tick, Is.EqualTo(-1));
            Assert.That(ArenaNativeConnection.LiveHandleCount, Is.EqualTo(nativeBaseline));
            yield return null;
            yield return null;
            Assert.That(ArenaAddressableAssets.LiveHandleCount, Is.EqualTo(assetBaseline));
        }

        private void CreateClient()
        {
            var root = new GameObject("Addressables lifecycle test");
            root.SetActive(false);
            client = root.AddComponent<KituArenaClient>();
            client.ConnectOnStart = false; client.DeviceInput = false; client.PauseOnFocusLoss = false;
            client.Backend = ArenaBackend.Embedded; client.NativeBridgeEnabled = false; client.NativeAutomaticTicks = false;
            client.BundledContentDirectory = directory;
            client.NativeStorageDirectory = Path.Combine(directory, "storage");
            root.SetActive(true);
        }
        private IEnumerator Ready() { yield return Until(() => client.Connected, "package/assets/native synchronization"); }
        private IEnumerator Until(Func<bool> condition, string operation)
        {
            double deadline = Time.realtimeSinceStartupAsDouble + 20;
            while (!condition())
            {
                if (Time.realtimeSinceStartupAsDouble > deadline) Assert.Fail(operation + ": " + client?.Message);
                yield return null;
            }
        }
        private void AssertOneView()
        {
            Assert.That(client.AssetsReady, Is.True);
            Assert.That(((JArray)client.AssetReport["loaded"]).Count, Is.EqualTo(4));
            string[] names = client.GetComponentsInChildren<Transform>(true).Select(value => value.name).ToArray();
            foreach (string name in new[] { "Arena presentation", "Arena Camera", "Arena Light" })
                Assert.That(names.Count(value => value == name), Is.EqualTo(1), name);
            Assert.That(client.GetComponents<ArenaWorldView>().Length, Is.EqualTo(1));
            Assert.That(client.GetComponent<ArenaGame>(), Is.Null);
        }
        private void MutateMapping(Action<JArray> change)
        {
            string path = Path.Combine(directory, "unity-assets.json");
            var mapping = JObject.Parse(File.ReadAllText(path));
            change((JArray)mapping["assets"]);
            byte[] bytes = new UTF8Encoding(false).GetBytes(mapping.ToString(Formatting.None));
            File.WriteAllBytes(path, bytes);
            string manifestPath = Path.Combine(directory, "package.json");
            var manifest = JObject.Parse(File.ReadAllText(manifestPath));
            var entry = (JObject)manifest["files"][0];
            entry["bytes"] = bytes.Length;
            using (var sha = SHA256.Create()) entry["sha256"] = BitConverter.ToString(sha.ComputeHash(bytes)).Replace("-", "").ToLowerInvariant();
            File.WriteAllText(manifestPath, manifest.ToString(Formatting.None), new UTF8Encoding(false));
            Assert.DoesNotThrow(() => ArenaContentPackage.Load(directory), "mutated package must pass byte/schema validation before the real asset lookup fails");
        }
        private static void RequireNative()
        {
            if (Application.platform != RuntimePlatform.OSXEditor && Application.platform != RuntimePlatform.OSXPlayer)
                Assert.Ignore("Native Arena ownership currently targets macOS.");
        }
    }
}
