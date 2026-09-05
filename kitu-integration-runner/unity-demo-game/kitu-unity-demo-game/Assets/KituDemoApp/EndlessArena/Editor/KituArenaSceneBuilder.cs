using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace UnityOnlyArena.Editor
{
    public static class KituArenaSceneBuilder
    {
        [MenuItem("Kitu/Build Kitu Arena Scene")]
        public static void Build()
        {
            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            new GameObject("Kitu Arena", typeof(KituArenaClient));
            EditorSceneManager.SaveScene(scene, "Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity");
        }
    }
}
