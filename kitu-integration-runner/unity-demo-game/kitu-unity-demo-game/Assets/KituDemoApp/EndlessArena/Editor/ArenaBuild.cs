using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace UnityOnlyArena.Editor
{
    public static class ArenaBuild
    {
        public const string ScenePath = "Assets/KituDemoApp/EndlessArena/EndlessArena.unity";

        [MenuItem("Kitu/Prepare Unity-only Endless Arena")]
        public static void Prepare()
        {
            if (!File.Exists(ScenePath))
            {
                Scene scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
                new GameObject("Endless Arena", typeof(ArenaGame));
                EditorSceneManager.SaveScene(scene, ScenePath);
            }
            var existing = EditorBuildSettings.scenes.Where(s => s.path != ScenePath);
            EditorBuildSettings.scenes = new[] { new EditorBuildSettingsScene(ScenePath, true) }.Concat(existing).ToArray();
            AssetDatabase.SaveAssets();
            Debug.Log("Arena scene ready: " + ScenePath);
        }

        [MenuItem("Kitu/Build Unity-only Endless Arena (macOS)")]
        public static void BuildMac()
        {
            Prepare();
            string destination = Path.GetFullPath(Path.Combine(Application.dataPath, "..", "Builds", "EndlessArena.app"));
            Directory.CreateDirectory(Path.GetDirectoryName(destination));
            BuildReport report = BuildPipeline.BuildPlayer(new BuildPlayerOptions
            {
                scenes = new[] { ScenePath },
                locationPathName = destination,
                target = BuildTarget.StandaloneOSX,
                options = BuildOptions.Development,
            });
            if (report.summary.result != BuildResult.Succeeded)
                throw new Exception("Arena build failed: " + report.summary.result);
            Debug.Log("Arena player built: " + destination);
        }
    }
}
