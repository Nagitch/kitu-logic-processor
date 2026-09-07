using System;
using System.IO;
using UnityEditor;
using UnityEngine;

namespace UnityOnlyArena.Editor
{
    /// <summary>Imports only the application-owned Apple Silicon native library.</summary>
    public sealed class KituArenaNativePlugin : AssetPostprocessor
    {
        public const string PluginPath = "Assets/Plugins/macOS/libkitu_demo_game_native.dylib";

        private void OnPreprocessAsset()
        {
            if (assetPath == PluginPath && assetImporter is PluginImporter plugin)
                Apply(plugin);
        }

        private static void Apply(PluginImporter plugin)
        {
            plugin.SetCompatibleWithAnyPlatform(false);
            plugin.SetCompatibleWithEditor(true);
            plugin.SetEditorData("CPU", "ARM64");
            plugin.SetEditorData("OS", "OSX");
            plugin.SetCompatibleWithPlatform(BuildTarget.StandaloneOSX, true);
            plugin.SetPlatformData(BuildTarget.StandaloneOSX, "CPU", "ARM64");
            plugin.SetCompatibleWithPlatform(BuildTarget.StandaloneWindows, false);
            plugin.SetCompatibleWithPlatform(BuildTarget.StandaloneWindows64, false);
            plugin.SetCompatibleWithPlatform(BuildTarget.StandaloneLinux64, false);
            plugin.isPreloaded = false;
        }

        [MenuItem("Kitu/Configure Arena Native Plugin (macOS ARM64)")]
        public static void Configure()
        {
            if (!File.Exists(PluginPath))
                throw new FileNotFoundException("Build the native plugin first with tools/build-arena-native-macos.py", PluginPath);
            AssetDatabase.ImportAsset(PluginPath, ImportAssetOptions.ForceSynchronousImport);
            var plugin = AssetImporter.GetAtPath(PluginPath) as PluginImporter;
            if (plugin == null || !plugin.isNativePlugin)
                throw new InvalidOperationException("Arena dylib was not imported as a native plugin");
            Apply(plugin);
            plugin.SaveAndReimport();
            if (plugin.GetCompatibleWithAnyPlatform() || !plugin.GetCompatibleWithEditor()
                || !plugin.GetCompatibleWithPlatform(BuildTarget.StandaloneOSX)
                || plugin.GetEditorData("CPU") != "ARM64"
                || plugin.GetEditorData("OS") != "OSX"
                || plugin.GetPlatformData(BuildTarget.StandaloneOSX, "CPU") != "ARM64")
                throw new InvalidOperationException("Native plugin import settings did not persist");
            Debug.Log("Arena native plugin verified: macOS Player and Editor, ARM64");
        }
    }
}
