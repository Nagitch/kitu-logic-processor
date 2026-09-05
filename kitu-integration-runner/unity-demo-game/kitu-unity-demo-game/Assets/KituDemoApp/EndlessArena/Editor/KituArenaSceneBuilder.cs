using System;
using System.IO;
using System.Linq;
using UnityEditor;
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
            string destination = Path.GetFullPath(Path.Combine(Application.dataPath, "..", "Builds", "KituEndlessArena.app"));
            Directory.CreateDirectory(Path.GetDirectoryName(destination));
            var report = BuildPipeline.BuildPlayer(new BuildPlayerOptions {
                scenes = new[] { ScenePath }, locationPathName = destination,
                target = BuildTarget.StandaloneOSX, options = BuildOptions.Development,
            });
            if (report.summary.result != BuildResult.Succeeded)
                throw new Exception("Kitu Arena build failed: " + report.summary.result);
        }
    }
}
