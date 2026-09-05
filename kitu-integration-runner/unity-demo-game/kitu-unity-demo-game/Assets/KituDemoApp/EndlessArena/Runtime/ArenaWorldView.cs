using System.Collections.Generic;
using UnityEngine;

namespace UnityOnlyArena
{
    public sealed class ArenaWorldView : MonoBehaviour
    {
        private readonly Dictionary<string, GameObject> objects = new Dictionary<string, GameObject>();
        private readonly Dictionary<Color, Material> materials = new Dictionary<Color, Material>();
        private readonly HashSet<string> live = new HashSet<string>();
        private readonly List<string> obsolete = new List<string>();
        private Material baseMaterial;
        private Transform stage, player, chest, portal;
        private LineRenderer shield;
        public Camera GameCamera { get; private set; }

        public void Initialize(ArenaSimulation model)
        {
            baseMaterial = Resources.Load<Material>("ArenaBase");
            stage = new GameObject("Arena presentation").transform;
            stage.SetParent(transform);
            Primitive("Floor", PrimitiveType.Cube, new Vector3(0, -.2f, 0), new Vector3(20, .4f, 20), new Color(.10f, .15f, .21f));
            for (int side = -1; side <= 1; side += 2)
            {
                Primitive("Wall", PrimitiveType.Cube, new Vector3(side * 10.2f, .4f, 0), new Vector3(.4f, .8f, 20.8f), new Color(.23f, .30f, .39f));
                Primitive("Wall", PrimitiveType.Cube, new Vector3(0, .4f, side * 10.2f), new Vector3(20, .8f, .4f), new Color(.23f, .30f, .39f));
            }
            player = Primitive("Player", PrimitiveType.Capsule, Vector3.up, new Vector3(.8f, .65f, .8f), new Color(.25f, .85f, 1f)).transform;
            var nose = Primitive("Aim marker", PrimitiveType.Cube, Vector3.zero, new Vector3(.16f, .2f, .5f), Color.white).transform;
            nose.SetParent(player, false);
            nose.localPosition = new Vector3(0, .35f, .65f);
            shield = Ring("Shield", new Color(.18f, .85f, 1f), 40);
            chest = Primitive("Supply chest", PrimitiveType.Cube, Point(ArenaSimulation.ChestPosition, .5f), new Vector3(1, 1, .8f), new Color(1f, .66f, .17f)).transform;
            portal = Ring("Next floor portal", new Color(.4f, 1f, .65f), 40).transform;
            portal.position = Point(ArenaSimulation.PortalPosition, .08f);
            SetRing(portal.GetComponent<LineRenderer>(), 1f);
            var cameraObject = new GameObject("Arena Camera", typeof(Camera), typeof(AudioListener));
            cameraObject.transform.SetParent(transform);
            GameCamera = cameraObject.GetComponent<Camera>();
            GameCamera.tag = "MainCamera";
            GameCamera.transform.SetPositionAndRotation(new Vector3(0, 25, 0), Quaternion.Euler(90, 0, 0));
            GameCamera.orthographic = true;
            GameCamera.clearFlags = CameraClearFlags.SolidColor;
            GameCamera.backgroundColor = new Color(.035f, .055f, .085f);
            GameCamera.nearClipPlane = .1f;
            GameCamera.farClipPlane = 50;
            var sun = new GameObject("Arena Light", typeof(Light));
            sun.transform.SetParent(transform);
            sun.transform.rotation = Quaternion.Euler(65, -25, 0);
            sun.GetComponent<Light>().type = LightType.Directional;
            sun.GetComponent<Light>().intensity = 1.3f;
            RenderSettings.ambientLight = new Color(.6f, .65f, .75f);
            Sync(model);
        }

        public void Sync(ArenaSimulation model)
        {
            if (GameCamera == null) return;
            GameCamera.rect = new Rect(0, .19f, 1, .65f);
            GameCamera.orthographicSize = 11.5f * Mathf.Max(1f, 1f / GameCamera.aspect);
            bool inRun = model.Phase != ArenaPhase.Opening;
            stage.gameObject.SetActive(inRun);
            player.position = Point(model.PlayerPosition, .8f);
            player.rotation = Quaternion.LookRotation(Point(model.AimDirection, 0));
            chest.gameObject.SetActive(inRun && model.ChestAvailable);
            portal.gameObject.SetActive(inRun && model.PortalAvailable);
            shield.gameObject.SetActive(inRun && HasShield(model.Inventory));
            shield.transform.position = Point(model.PlayerPosition, .12f);
            SetRing(shield, .85f);
            live.Clear();
            foreach (var enemy in model.Enemies)
            {
                string id = "enemy-" + enemy.Id;
                Color color = enemy.Kind == ArenaEnemyKind.Boss ? new Color(.83f, .3f, 1f) :
                    enemy.Kind == ArenaEnemyKind.Shooter ? new Color(1f, .69f, .2f) :
                    enemy.Kind == ArenaEnemyKind.Heavy ? new Color(.75f, .22f, .23f) : new Color(1f, .4f, .3f);
                var obj = Dynamic(id, enemy.Kind == ArenaEnemyKind.Shooter ? PrimitiveType.Cube : PrimitiveType.Capsule, color);
                obj.transform.position = Point(enemy.Position, .7f);
                obj.transform.localScale = new Vector3(enemy.Radius * 2, enemy.Kind == ArenaEnemyKind.Boss ? 1f : .6f, enemy.Radius * 2);
                if (enemy.Kind == ArenaEnemyKind.Boss && enemy.BossState == ArenaBossState.Telegraph)
                {
                    var tell = DynamicRing("tell-" + enemy.Id, new Color(1f, .2f, .4f));
                    tell.transform.position = Point(enemy.Position, .15f);
                    SetRing(tell, 3f);
                }
            }
            foreach (var bullet in model.Projectiles)
            {
                var obj = Dynamic("shot-" + bullet.Id, PrimitiveType.Sphere, bullet.EnemyOwned ? new Color(1f, .25f, .25f) : new Color(.9f, 1f, .65f));
                obj.transform.position = Point(bullet.Position, .5f);
                obj.transform.localScale = Vector3.one * .32f;
            }
            foreach (var grenade in model.Grenades)
            {
                var obj = Dynamic("grenade-" + grenade.Id, PrimitiveType.Sphere, new Color(.6f, 1f, .2f));
                obj.transform.position = Point(grenade.Position, .5f + Mathf.Sin(Mathf.Clamp01(1f - grenade.Remaining / .5f) * Mathf.PI) * 2);
                obj.transform.localScale = Vector3.one * .5f;
            }
            foreach (var effect in model.Effects)
            {
                var ring = DynamicRing("effect-" + effect.Id, effect.Kind == ArenaEffectKind.Explosion ? new Color(1f, .8f, .25f) : Color.white);
                ring.transform.position = Point(effect.Position, .15f);
                if (effect.Kind == ArenaEffectKind.Slash)
                {
                    ring.positionCount = 15;
                    ring.loop = true;
                    ring.SetPosition(0, Vector3.zero);
                    float yaw = Mathf.Atan2(effect.Direction.x, effect.Direction.y);
                    for (int i = 1; i < 15; i++)
                    {
                        float angle = yaw + Mathf.Lerp(-Mathf.PI / 4, Mathf.PI / 4, (i - 1) / 13f);
                        ring.SetPosition(i, new Vector3(Mathf.Sin(angle), 0, Mathf.Cos(angle)) * effect.Radius);
                    }
                }
                else SetRing(ring, effect.Radius);
            }
            obsolete.Clear();
            foreach (var item in objects) if (!live.Contains(item.Key)) obsolete.Add(item.Key);
            foreach (string key in obsolete) { Destroy(objects[key]); objects.Remove(key); }
        }

        private static bool HasShield(ArenaInventory inventory)
        {
            for (int i = 2; i < 4; i++)
                if (inventory.Equipment[i] != null && inventory.Equipment[i].Kind == ItemKind.Shield && inventory.Equipment[i].Shield > 0) return true;
            return false;
        }

        private GameObject Dynamic(string id, PrimitiveType shape, Color color)
        {
            live.Add(id);
            if (!objects.TryGetValue(id, out var obj))
            {
                obj = Primitive(id, shape, Vector3.zero, Vector3.one, color);
                objects.Add(id, obj);
            }
            return obj;
        }

        private LineRenderer DynamicRing(string id, Color color)
        {
            live.Add(id);
            if (!objects.TryGetValue(id, out var obj)) { obj = Ring(id, color, 40).gameObject; objects.Add(id, obj); }
            return obj.GetComponent<LineRenderer>();
        }

        private GameObject Primitive(string name, PrimitiveType type, Vector3 position, Vector3 scale, Color color)
        {
            var obj = GameObject.CreatePrimitive(type);
            obj.name = name;
            obj.transform.SetParent(stage);
            obj.transform.position = position;
            obj.transform.localScale = scale;
            Destroy(obj.GetComponent<Collider>());
            obj.GetComponent<Renderer>().sharedMaterial = Material(color);
            return obj;
        }

        private LineRenderer Ring(string name, Color color, int count)
        {
            var obj = new GameObject(name, typeof(LineRenderer));
            obj.transform.SetParent(stage);
            var ring = obj.GetComponent<LineRenderer>();
            ring.useWorldSpace = false;
            ring.loop = true;
            ring.positionCount = count;
            ring.startWidth = ring.endWidth = .075f;
            ring.sharedMaterial = Material(color);
            return ring;
        }

        private static void SetRing(LineRenderer ring, float radius)
        {
            for (int i = 0; i < ring.positionCount; i++)
            {
                float angle = i * Mathf.PI * 2 / ring.positionCount;
                ring.SetPosition(i, new Vector3(Mathf.Sin(angle), 0, Mathf.Cos(angle)) * radius);
            }
        }

        private Material Material(Color color)
        {
            if (!materials.TryGetValue(color, out var material))
            {
                material = new Material(baseMaterial);
                material.color = color;
                materials.Add(color, material);
            }
            return material;
        }

        public static Vector3 Point(Vector2 value, float height) => new Vector3(value.x, height, value.y);

        private void OnDestroy()
        {
            foreach (var material in materials.Values) Destroy(material);
        }
    }
}
