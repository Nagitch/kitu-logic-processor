using Kitu.Runtime;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace Kitu.Editor
{
    public static class KituNetworkDemoSceneBuilder
    {
        private const string ScenePath = "Assets/Scenes/KituNetworkDemo.unity";

        [MenuItem("Kitu/Build Network Runtime Demo Scene")]
        public static void BuildScene()
        {
            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            scene.name = "KituNetworkDemo";

            var camera = new GameObject("Main Camera");
            camera.tag = "MainCamera";
            camera.transform.position = new Vector3(0f, 7f, -8f);
            camera.transform.rotation = Quaternion.Euler(55f, 0f, 0f);
            camera.AddComponent<Camera>();
            camera.AddComponent<AudioListener>();

            var light = new GameObject("Directional Light");
            light.transform.rotation = Quaternion.Euler(50f, -30f, 0f);
            var directional = light.AddComponent<Light>();
            directional.type = LightType.Directional;
            directional.intensity = 1.2f;

            var player = GameObject.CreatePrimitive(PrimitiveType.Capsule);
            player.name = "Player View";
            player.transform.position = Vector3.zero;

            var ground = GameObject.CreatePrimitive(PrimitiveType.Plane);
            ground.name = "Ground";
            ground.transform.localScale = new Vector3(2f, 1f, 2f);

            var runtime = new GameObject("Kitu Network Runtime");
            var client = runtime.AddComponent<KituNetworkRuntimeClient>();
            var controller = runtime.AddComponent<KituNetworkPlayerController>();
            runtime.AddComponent<KituWorldObjectPresenter>();

            var serializedController = new SerializedObject(controller);
            serializedController.FindProperty("playerView").objectReferenceValue = player.transform;
            serializedController.ApplyModifiedPropertiesWithoutUndo();

            Selection.activeObject = client;
            EditorSceneManager.SaveScene(scene, ScenePath);
        }
    }
}
