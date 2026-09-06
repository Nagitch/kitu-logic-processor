using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace UnityOnlyArena.Editor
{
    public static class KituArenaSceneBuilder
    {
        public const string ScenePath = "Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity";

        [MenuItem("Kitu/Build Kitu Arena Scene")]
        public static void Build()
        {
            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            new GameObject("Kitu Arena", typeof(KituArenaClient));
            EditorSceneManager.SaveScene(scene, ScenePath);
        }

        [MenuItem("Kitu/Prepare Endless Arena (Kitu)")]
        public static void PrepareDefault()
        {
            if (!File.Exists(ScenePath)) Build();
            var rest = EditorBuildSettings.scenes.Where(s => s.path != ScenePath);
            EditorBuildSettings.scenes = new[] { new EditorBuildSettingsScene(ScenePath, true) }.Concat(rest).ToArray();
            AssetDatabase.SaveAssets();
        }

        [MenuItem("Kitu/Build Endless Arena (Kitu, macOS)")]
        public static void BuildMac()
        {
            PrepareDefault();
            KituArenaNativePlugin.Configure();
            string destination = Environment.GetEnvironmentVariable("KITU_ARENA_PLAYER_PATH");
            if (string.IsNullOrWhiteSpace(destination))
                destination = Path.Combine(Application.dataPath, "..", "Builds", "KituEndlessArena.app");
            destination = Path.GetFullPath(destination);
            if (!destination.EndsWith(".app", StringComparison.Ordinal))
                throw new ArgumentException("KITU_ARENA_PLAYER_PATH must end with .app");
            Directory.CreateDirectory(Path.GetDirectoryName(destination));
            string platform = BuildPipeline.GetBuildTargetName(BuildTarget.StandaloneOSX);
            string previousArchitecture = EditorUserBuildSettings.GetPlatformSettings(platform, "Architecture");
            var previousBackend = PlayerSettings.GetScriptingBackend(NamedBuildTarget.Standalone);
            bool previousBackground = PlayerSettings.runInBackground;
            string previousProductName = PlayerSettings.productName;
            try
            {
                EditorUserBuildSettings.SetPlatformSettings(platform, "Architecture", "ARM64");
                PlayerSettings.SetScriptingBackend(NamedBuildTarget.Standalone, ScriptingImplementation.Mono2x);
                PlayerSettings.runInBackground = true;
                PlayerSettings.productName = "Kitu Endless Arena";
                var report = BuildPipeline.BuildPlayer(new BuildPlayerOptions {
                    scenes = new[] { ScenePath }, locationPathName = destination,
                    target = BuildTarget.StandaloneOSX, options = BuildOptions.Development,
                });
                var evidence = new NativeBuildEvidence {
                    unityVersion = Application.unityVersion, playerPath = destination,
                    scene = ScenePath, architecture = "arm64", scriptingBackend = "Mono2x",
                    pluginPath = KituArenaNativePlugin.PluginPath,
                    result = report.summary.result.ToString(), errors = report.summary.totalErrors,
                    warnings = report.summary.totalWarnings, totalBytes = report.summary.totalSize,
                    elapsedSeconds = report.summary.totalTime.TotalSeconds,
                };
                string reportPath = Environment.GetEnvironmentVariable("KITU_ARENA_BUILD_REPORT");
                if (!string.IsNullOrWhiteSpace(reportPath))
                {
                    reportPath = Path.GetFullPath(reportPath);
                    Directory.CreateDirectory(Path.GetDirectoryName(reportPath));
                    File.WriteAllText(reportPath, JsonUtility.ToJson(evidence, true));
                }
                if (report.summary.result != BuildResult.Succeeded)
                    throw new Exception("Kitu Arena build failed: " + report.summary.result);
                Debug.Log("Embedded native Arena player built: " + destination);
            }
            finally
            {
                EditorUserBuildSettings.SetPlatformSettings(platform, "Architecture", previousArchitecture);
                PlayerSettings.SetScriptingBackend(NamedBuildTarget.Standalone, previousBackend);
                PlayerSettings.runInBackground = previousBackground;
                PlayerSettings.productName = previousProductName;
                AssetDatabase.SaveAssets();
            }
        }

        [Serializable]
        private sealed class NativeBuildEvidence
        {
            public string unityVersion, playerPath, scene, architecture, scriptingBackend, pluginPath, result;
            public int errors, warnings;
            public ulong totalBytes;
            public double elapsedSeconds;
        }
    }
}
