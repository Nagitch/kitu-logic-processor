using System.Collections.Generic;
using UnityEngine;

namespace UnityOnlyArena
{
    public sealed class ArenaWorldView : MonoBehaviour
    {
        [SerializeField, Range(15f, 85f)] private float cameraPitch = 56f;
        [SerializeField, Range(-180f, 180f)] private float cameraYaw = -72f;
        [SerializeField, Min(2f)] private float cameraDistance = 55f;
        [SerializeField, Range(5f, 90f)] private float cameraFieldOfView = 13.1f;
        private readonly Dictionary<string, GameObject> objects = new Dictionary<string, GameObject>();
        private readonly Dictionary<Color, Material> materials = new Dictionary<Color, Material>();
        private readonly HashSet<string> live = new HashSet<string>();
        private readonly List<string> obsolete = new List<string>();
        private Material baseMaterial;
        private Transform stage, player, chest, portal;
        private LineRenderer shield;
        private bool timelineDriven;
        private ArenaPresentationState presentation;
        public Camera GameCamera { get; private set; }

        public void Initialize(ArenaSimulation model) => Initialize(new ReferenceView(model));
        public void Initialize(ArenaReferenceState state) => Initialize(new ProjectionView(state));
        public void Sync(ArenaSimulation model) => Sync(new ReferenceView(model));
        public void Sync(ArenaReferenceState state) => Sync(new ProjectionView(state));
        public void UseTimelinePresentation() { timelineDriven = true; }
        public void SyncPresentation(ArenaReferenceState state, ArenaPresentationState value)
        {
            presentation = value;
            Sync(state);
        }

        private void Initialize(IView model)
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
            GameCamera.orthographic = false;
            GameCamera.clearFlags = CameraClearFlags.SolidColor;
            GameCamera.backgroundColor = new Color(.035f, .055f, .085f);
            GameCamera.nearClipPlane = .1f;
            var sun = new GameObject("Arena Light", typeof(Light));
            sun.transform.SetParent(transform);
            sun.transform.rotation = Quaternion.Euler(65, -25, 0);
            sun.GetComponent<Light>().type = LightType.Directional;
            sun.GetComponent<Light>().intensity = 1.3f;
            RenderSettings.ambientLight = new Color(.6f, .65f, .75f);
            Sync(model);
        }

        private void Sync(IView model)
        {
            if (GameCamera == null) return;
            GameCamera.rect = new Rect(0, .19f, 1, .65f);
            var cameraRotation = Quaternion.Euler(cameraPitch, cameraYaw, 0f);
            // Follow without arena-edge clamping or whole-floor zoom. This also snaps
            // directly to the entrance on a floor change or retry, before rendering.
            GameCamera.transform.SetPositionAndRotation(Point(model.PlayerPosition, .8f) +
                cameraRotation * Vector3.back * cameraDistance, cameraRotation);
            GameCamera.fieldOfView = cameraFieldOfView;
            GameCamera.farClipPlane = Mathf.Max(100f, cameraDistance + 40f);
            bool inRun = model.Phase != ArenaPhase.Opening;
            stage.gameObject.SetActive(inRun);
            player.position = Point(model.PlayerPosition, .8f);
            player.rotation = Quaternion.LookRotation(Point(model.AimDirection, 0));
            chest.gameObject.SetActive(inRun && model.ChestAvailable);
            portal.gameObject.SetActive(inRun && model.PortalAvailable);
            shield.gameObject.SetActive(inRun && model.HasShield);
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
                    var cue = timelineDriven ? presentation?.Boss(enemy.Id) : null;
                    if (!timelineDriven || (cue != null && cue.intensity > 0))
                    {
                        var tell = DynamicRing("tell-" + enemy.Id, new Color(1f, .2f, .4f));
                        tell.transform.position = Point(enemy.Position, .15f);
                        tell.startWidth = tell.endWidth = timelineDriven ? .025f + cue.intensity * .1f : .075f;
                        SetRing(tell, timelineDriven ? cue.radius : 3f);
                    }
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

        // Read-only adapters keep rendering shared without running reference rules in the Kitu client.
        private interface IView
        {
            ArenaPhase Phase { get; }
            Vector2 PlayerPosition { get; }
            Vector2 AimDirection { get; }
            bool ChestAvailable { get; }
            bool PortalAvailable { get; }
            bool HasShield { get; }
            IEnumerable<ArenaEnemy> Enemies { get; }
            IEnumerable<ArenaProjectile> Projectiles { get; }
            IEnumerable<ArenaGrenade> Grenades { get; }
            IEnumerable<ArenaEffect> Effects { get; }
        }

        private sealed class ReferenceView : IView
        {
            private readonly ArenaSimulation value;
            public ReferenceView(ArenaSimulation value) { this.value = value; }
            public ArenaPhase Phase => value.Phase;
            public Vector2 PlayerPosition => value.PlayerPosition;
            public Vector2 AimDirection => value.AimDirection;
            public bool ChestAvailable => value.ChestAvailable;
            public bool PortalAvailable => value.PortalAvailable;
            public IEnumerable<ArenaEnemy> Enemies => value.Enemies;
            public IEnumerable<ArenaProjectile> Projectiles => value.Projectiles;
            public IEnumerable<ArenaGrenade> Grenades => value.Grenades;
            public IEnumerable<ArenaEffect> Effects => value.Effects;
            public bool HasShield {
                get {
                    for (int i = 2; i < 4; i++)
                        if (value.Inventory.Equipment[i] != null && value.Inventory.Equipment[i].Kind == ItemKind.Shield && value.Inventory.Equipment[i].Shield > 0) return true;
                    return false;
                }
            }
        }

        private sealed class ProjectionView : IView
        {
            private readonly ArenaReferenceState value;
            public ProjectionView(ArenaReferenceState value) { this.value = value; }
            public ArenaPhase Phase => (ArenaPhase)value.phase;
            public Vector2 PlayerPosition => value.playerPosition;
            public Vector2 AimDirection => value.aimDirection;
            public bool ChestAvailable => value.chestAvailable;
            public bool PortalAvailable => value.portalAvailable;
            public IEnumerable<ArenaEnemy> Enemies => value.enemies ?? System.Array.Empty<ArenaEnemy>();
            public IEnumerable<ArenaProjectile> Projectiles => value.projectiles ?? System.Array.Empty<ArenaProjectile>();
            public IEnumerable<ArenaGrenade> Grenades => value.grenades ?? System.Array.Empty<ArenaGrenade>();
            public IEnumerable<ArenaEffect> Effects => value.effects ?? System.Array.Empty<ArenaEffect>();
            public bool HasShield {
                get {
                    if (value.inventory?.equipment == null) return false;
                    for (int i = 2; i < value.inventory.equipment.Length; i++)
                        if (value.inventory.equipment[i].kind == (int)ItemKind.Shield && value.inventory.equipment[i].shield > 0) return true;
                    return false;
                }
            }
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
