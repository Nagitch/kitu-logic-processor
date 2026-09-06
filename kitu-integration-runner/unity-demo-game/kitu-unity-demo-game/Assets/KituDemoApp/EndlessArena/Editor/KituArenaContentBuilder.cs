using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEditor;
using UnityEditor.AddressableAssets;
using UnityEditor.AddressableAssets.Build;
using UnityEditor.AddressableAssets.Build.DataBuilders;
using UnityEditor.AddressableAssets.Settings;
using UnityEditor.AddressableAssets.Settings.GroupSchemas;
using UnityEngine;
using UnityEngine.AddressableAssets;
using UnityEngine.AddressableAssets.ResourceLocators;
using UnityEngine.ResourceManagement.ResourceProviders;

namespace UnityOnlyArena.Editor
{
    /// <summary>Builds the Kitu view's local assets without modifying the procedural reference.</summary>
    public static class KituArenaContentBuilder
    {
        public const string AssetDirectory = "Assets/KituDemoApp/EndlessArena/AddressableContent";
        public const string GroupName = "Arena Local";
        public const string PackageVersion = "2.11.2";
        private const string ReferenceMaterial = "Assets/KituDemoApp/EndlessArena/Resources/ArenaBase.mat";
        private const string RuntimePrefix = "{UnityEngine.AddressableAssets.Addressables.RuntimePath}/";
        private static readonly string[] Keys = { "arena/material/base", "arena/primitive/cube", "arena/primitive/capsule", "arena/primitive/sphere" };
        private static readonly string[] Names = { "ArenaBase.mat", "Cube.prefab", "Capsule.prefab", "Sphere.prefab" };
        private static readonly PrimitiveType[] Shapes = { PrimitiveType.Cube, PrimitiveType.Capsule, PrimitiveType.Sphere };

        [MenuItem("Kitu/Prepare Arena Addressables")]
        public static void Prepare()
        {
            var version = UnityEditor.PackageManager.PackageInfo.FindForAssembly(typeof(Addressables).Assembly)?.version;
            if (version != PackageVersion) throw new InvalidOperationException("Arena requires Addressables " + PackageVersion + ", found " + version);
            Folder(AssetDirectory);
            string materialPath = AssetDirectory + "/" + Names[0];
            if (AssetDatabase.LoadAssetAtPath<Material>(materialPath) == null && !AssetDatabase.CopyAsset(ReferenceMaterial, materialPath))
                throw new InvalidOperationException("Could not copy Arena's reference material");
            var material = AssetDatabase.LoadAssetAtPath<Material>(materialPath);
            var reference = AssetDatabase.LoadAssetAtPath<Material>(ReferenceMaterial);
            if (material == null || reference == null || material.shader != reference.shader)
                throw new InvalidOperationException("Arena Addressable material must preserve the reference shader");
            for (int i = 0; i < Shapes.Length; i++) PreparePrefab(AssetDirectory + "/" + Names[i + 1], Shapes[i], material);

            var settings = AddressableAssetSettingsDefaultObject.GetSettings(true);
            if (settings == null) throw new InvalidOperationException("Addressables settings were not created");
            RequireLocalProfiles(settings);
            bool settingsChanged = settings.BuildRemoteCatalog || !settings.DisableCatalogUpdateOnStartup;
            settings.BuildRemoteCatalog = false;
            settings.DisableCatalogUpdateOnStartup = true;
            // Keep the package's native binary catalog; toggling JSON catalogs
            // changes scripting defines and can trigger an unrelated domain reload.
            if (settings.EnableJsonCatalog) throw new InvalidOperationException("Arena's packed build requires the default binary catalog");
            var group = settings.FindGroup(GroupName) ?? settings.CreateGroup(GroupName, false, false, false, null,
                typeof(BundledAssetGroupSchema), typeof(ContentUpdateGroupSchema));
            var schema = group.GetSchema<BundledAssetGroupSchema>();
            if (schema == null) throw new InvalidOperationException("Arena Local has no bundled asset schema");
            if (schema.BuildPath.GetName(settings) != AddressableAssetSettings.kLocalBuildPath)
                schema.BuildPath.SetVariableByName(settings, AddressableAssetSettings.kLocalBuildPath);
            if (schema.LoadPath.GetName(settings) != AddressableAssetSettings.kLocalLoadPath)
                schema.LoadPath.SetVariableByName(settings, AddressableAssetSettings.kLocalLoadPath);
            schema.UseDefaultSchemaSettings = false;
            schema.BundleMode = BundledAssetGroupSchema.BundlePackingMode.PackTogether;
            schema.Compression = BundledAssetGroupSchema.BundleCompressionMode.LZ4;
            schema.UseUnityWebRequestForLocalBundles = false;
            schema.IncludeInBuild = true;
            schema.IncludeAddressInCatalog = true;
            for (int i = 0; i < Keys.Length; i++)
            {
                string guid = AssetDatabase.AssetPathToGUID(AssetDirectory + "/" + Names[i]);
                if (string.IsNullOrEmpty(guid)) throw new InvalidOperationException("Missing Addressable source " + Names[i]);
                settings.CreateOrMoveEntry(guid, group).address = Keys[i];
            }
            if (group.entries.Count != Keys.Length || group.entries.Any(entry => !Keys.Contains(entry.address)))
                throw new InvalidOperationException("Arena Local must contain exactly the four supported visual assets");
            int packed = settings.DataBuilders.FindIndex(builder => builder is BuildScriptPackedMode);
            if (packed < 0) throw new InvalidOperationException("Addressables has no packed content builder");
            settings.ActivePlayerDataBuilderIndex = packed;
            if (settingsChanged) settings.SetDirty(AddressableAssetSettings.ModificationEvent.BatchModification, null, true, true);
            AssetDatabase.SaveAssets();
            Debug.Log("Arena local Addressables prepared: " + string.Join(", ", Keys));
        }

        /// <summary>Runs the packed builder and verifies its actual catalog and local artifact paths.</summary>
        public static JObject BuildContent()
        {
            Prepare();
            var package = ArenaContentPackage.Load(Path.Combine(Application.streamingAssetsPath, "KituArena"));
            ValidateRequestedKeys(package);
            AddressableAssetSettings.BuildPlayerContent(out AddressablesPlayerBuildResult result);
            if (result == null || !string.IsNullOrEmpty(result.Error))
                throw new InvalidOperationException("Arena Addressables build failed: " + result?.Error);
            string buildRoot = Path.GetFullPath(Addressables.BuildPath);
            string settingsPath = Path.GetFullPath(result.OutputPath);
            var catalogs = Directory.GetFiles(buildRoot, "catalog*.bin", SearchOption.AllDirectories);
            if (catalogs.Length != 1) throw new InvalidOperationException("Arena build must produce exactly one local binary catalog");
            var locations = ExtractLocations(catalogs[0]);
            foreach (var entry in package.Keys)
            {
                var location = locations.OfType<JObject>().SingleOrDefault(value => (string)value["key"] == entry.Value);
                string expectedType = entry.Key == "baseMaterial" ? typeof(Material).ToString() : typeof(GameObject).ToString();
                if (location == null || (string)location["type"] != expectedType)
                    throw new InvalidOperationException("Packed catalog does not contain the requested key/type: " + entry.Value);
            }
            var artifacts = new JArray();
            foreach (string path in Directory.GetFiles(buildRoot, "*", SearchOption.AllDirectories).OrderBy(path => path, StringComparer.Ordinal))
            {
                if (!path.EndsWith(".bundle", StringComparison.Ordinal) && !path.EndsWith(".bin", StringComparison.Ordinal) &&
                    !path.EndsWith(".json", StringComparison.Ordinal) && !path.EndsWith(".hash", StringComparison.Ordinal)) continue;
                var file = FileArtifact(path);
                file["relativePath"] = Path.GetRelativePath(buildRoot, path).Replace('\\', '/');
                artifacts.Add(file);
            }
            if (!artifacts.Any(file => ((string)file["relativePath"]).EndsWith(".bundle", StringComparison.Ordinal)))
                throw new InvalidOperationException("Arena packed build produced no local AssetBundle");
            var report = new JObject {
                ["packageVersion"] = PackageVersion, ["group"] = GroupName,
                ["builder"] = nameof(BuildScriptPackedMode), ["catalogFormat"] = "binary",
                ["remoteCatalog"] = false, ["durationSeconds"] = result.Duration,
                ["locationCount"] = result.LocationCount, ["package"] = package.Report,
                ["settings"] = FileArtifact(settingsPath), ["catalog"] = FileArtifact(catalogs[0]),
                ["locations"] = locations, ["artifacts"] = artifacts,
            };
            string destination = Environment.GetEnvironmentVariable("KITU_ARENA_CONTENT_BUILD_REPORT");
            if (!string.IsNullOrWhiteSpace(destination))
            {
                destination = Path.GetFullPath(destination);
                Directory.CreateDirectory(Path.GetDirectoryName(destination));
                File.WriteAllText(destination, report.ToString(Formatting.Indented) + "\n");
            }
            return report;
        }

        [MenuItem("Kitu/Build Arena Addressable Content")]
        public static void BuildLocal() { BuildContent(); }

        private static void ValidateRequestedKeys(ArenaContentPackage package)
        {
            var group = AddressableAssetSettingsDefaultObject.Settings.FindGroup(GroupName);
            foreach (var requested in package.Keys)
            {
                var entry = group.entries.SingleOrDefault(value => value.address == requested.Value);
                Type expected = requested.Key == "baseMaterial" ? typeof(Material) : typeof(GameObject);
                if (entry == null || AssetDatabase.LoadAssetAtPath(entry.AssetPath, expected) == null)
                    throw new InvalidOperationException("The package requests an unavailable Addressable key/type: " + requested.Value);
            }
        }

        private static JArray ExtractLocations(string catalog)
        {
#if ENABLE_JSON_CATALOG
            throw new InvalidOperationException("Arena requires binary catalog support; remove ENABLE_JSON_CATALOG before building");
#else
            string temporary = Path.GetTempFileName();
            try
            {
                ContentCatalogData.ExtractBinaryCatalog(catalog, temporary);
                var locations = new JArray();
                JObject entry = null;
                foreach (string line in File.ReadAllLines(temporary))
                {
                    if (!line.StartsWith("\t", StringComparison.Ordinal))
                    {
                        entry = line.Contains(" -> ") ? null : new JObject { ["key"] = line };
                        if (entry != null) locations.Add(entry);
                    }
                    else if (entry != null)
                    {
                        if (line.StartsWith("\tResourceType: ", StringComparison.Ordinal)) entry["type"] = line.Substring(15);
                        else if (line.StartsWith("\tProviderId: ", StringComparison.Ordinal)) entry["provider"] = line.Substring(13);
                        else if (line.StartsWith("\tInternalId: ", StringComparison.Ordinal)) entry["internalId"] = line.Substring(13);
                    }
                }
                foreach (JObject location in locations)
                {
                    string provider = (string)location["provider"], id = (string)location["internalId"];
                    if (provider == typeof(AssetBundleProvider).FullName)
                    {
                        if (id == null || !id.StartsWith(RuntimePrefix, StringComparison.Ordinal) || id.Contains(".."))
                            throw new InvalidOperationException("Arena bundle has a nonlocal load path: " + id);
                    }
                    else if (provider != typeof(BundledAssetProvider).FullName)
                        throw new InvalidOperationException("Arena catalog uses an unexpected asset provider: " + provider);
                }
                return locations;
            }
            finally { File.Delete(temporary); }
#endif
        }

        private static JObject FileArtifact(string path)
        {
            path = Path.GetFullPath(path);
            using (var sha = SHA256.Create())
            using (var file = File.OpenRead(path))
                return new JObject { ["path"] = path, ["bytes"] = file.Length,
                    ["sha256"] = BitConverter.ToString(sha.ComputeHash(file)).Replace("-", "").ToLowerInvariant() };
        }

        private static void RequireLocalProfiles(AddressableAssetSettings settings)
        {
            if (settings.profileSettings.GetValueByName(settings.activeProfileId, AddressableAssetSettings.kLocalBuildPath) != AddressableAssetSettings.kLocalBuildPathValue ||
                settings.profileSettings.GetValueByName(settings.activeProfileId, AddressableAssetSettings.kLocalLoadPath) != AddressableAssetSettings.kLocalLoadPathValue)
                throw new InvalidOperationException("Arena requires the standard local Addressables build/load paths");
        }

        private static void PreparePrefab(string path, PrimitiveType shape, Material material)
        {
            var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(path);
            if (prefab == null)
            {
                var source = GameObject.CreatePrimitive(shape);
                try
                {
                    source.name = shape.ToString();
                    UnityEngine.Object.DestroyImmediate(source.GetComponent<Collider>());
                    source.GetComponent<Renderer>().sharedMaterial = material;
                    prefab = PrefabUtility.SaveAsPrefabAsset(source, path);
                }
                finally { UnityEngine.Object.DestroyImmediate(source); }
            }
            if (prefab == null || prefab.GetComponentsInChildren<Collider>(true).Length != 0 ||
                prefab.GetComponentsInChildren<MonoBehaviour>(true).Length != 0 ||
                prefab.GetComponentsInChildren<MeshRenderer>(true).Length != 1 ||
                prefab.GetComponent<Renderer>()?.sharedMaterial != material)
                throw new InvalidOperationException("Arena prefab must contain only the original visual primitive: " + path);
        }

        private static void Folder(string path)
        {
            if (AssetDatabase.IsValidFolder(path)) return;
            string parent = Path.GetDirectoryName(path).Replace('\\', '/');
            Folder(parent);
            AssetDatabase.CreateFolder(parent, Path.GetFileName(path));
        }
    }
}
