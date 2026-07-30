using UnityEngine;
using UnityEngine.InputSystem;
using UnityEngine.SceneManagement;

namespace UnityOnlyActionRpg
{
    public sealed class ActionRpgGameController : MonoBehaviour
    {
        [Header("Scene References")]
        [SerializeField] private ActionRpgPlayerController player;
        [SerializeField] private GameObject enemyTemplate;

        [Header("Gameplay Rules")]
        [SerializeField, Min(1)] private int targetKills = 3;
        [SerializeField, Min(1)] private int experiencePerKill = 10;
        [SerializeField, Min(1)] private int baseExperienceToLevel = 20;

        private static readonly Vector3[] EnemySpawnPositions =
        {
            new(-4f, 1f, 4.5f),
            new(0f, 1f, 6f),
            new(4f, 1f, 4.5f),
        };

        public ActionRpgPlayerController Player => player;
        public bool IsRunning { get; private set; }
        public bool IsComplete { get; private set; }
        public int EnemiesDefeated { get; private set; }
        public int TargetKills => targetKills;
        public int Potions { get; private set; }
        public int ItemsCollected { get; private set; }
        public int Level { get; private set; }
        public int Experience { get; private set; }
        public int ExperienceToNextLevel => baseExperienceToLevel * Level;

        public string ObjectiveText =>
            $"Defeat enemies: {EnemiesDefeated}/{targetKills}  |  Collect a potion: {(ItemsCollected > 0 ? "Done" : "Not yet")}";

        private void Awake()
        {
            if (player == null)
            {
                player = FindFirstObjectByType<ActionRpgPlayerController>();
            }

            if (enemyTemplate == null)
            {
                ActionRpgEnemy[] candidates = FindObjectsByType<ActionRpgEnemy>(
                    FindObjectsInactive.Include,
                    FindObjectsSortMode.None);
                foreach (ActionRpgEnemy candidate in candidates)
                {
                    if (candidate.name == "Enemy Template")
                    {
                        enemyTemplate = candidate.gameObject;
                        break;
                    }
                }
            }
        }

        private void Start()
        {
            Level = 1;
            Experience = 0;
            EnemiesDefeated = 0;
            Potions = 0;
            ItemsCollected = 0;
            IsComplete = false;
            IsRunning = player != null;

            SpawnInitialEnemies();
        }

        private void Update()
        {
            if ((IsComplete || !IsRunning) && Keyboard.current != null && Keyboard.current.rKey.wasPressedThisFrame)
            {
                SceneManager.LoadScene(SceneManager.GetActiveScene().path);
            }
        }

        public void RegisterEnemyDefeated(Vector3 position)
        {
            if (!IsRunning)
            {
                return;
            }

            EnemiesDefeated++;
            AddExperience(experiencePerKill);
            SpawnPotion(position);
            EvaluateObjective();
        }

        public void CollectPotion()
        {
            if (!IsRunning)
            {
                return;
            }

            Potions++;
            ItemsCollected++;
            EvaluateObjective();
        }

        public bool TryConsumePotion()
        {
            if (!IsRunning || Potions <= 0)
            {
                return false;
            }

            Potions--;
            return true;
        }

        public void RegisterPlayerDeath()
        {
            IsRunning = false;
            IsComplete = false;
        }

        private void AddExperience(int amount)
        {
            Experience += amount;

            while (Experience >= ExperienceToNextLevel)
            {
                Experience -= ExperienceToNextLevel;
                Level++;
                player.ApplyLevelUp();
            }
        }

        private void EvaluateObjective()
        {
            if (EnemiesDefeated < targetKills || ItemsCollected < 1)
            {
                return;
            }

            IsComplete = true;
            IsRunning = false;
        }

        private void SpawnInitialEnemies()
        {
            if (enemyTemplate == null)
            {
                Debug.LogError("Unity-only gameplay demo: Enemy Template is not assigned.");
                IsRunning = false;
                return;
            }

            enemyTemplate.SetActive(false);
            Transform parent = GameObject.Find("Enemies")?.transform;

            for (int i = 0; i < targetKills; i++)
            {
                Vector3 position = EnemySpawnPositions[i % EnemySpawnPositions.Length];
                GameObject enemy = Instantiate(enemyTemplate, position, Quaternion.identity, parent);
                enemy.name = $"Enemy {i + 1}";
                enemy.SetActive(true);
            }
        }

        private void SpawnPotion(Vector3 position)
        {
            GameObject pickup = GameObject.CreatePrimitive(PrimitiveType.Sphere);
            pickup.name = "Potion Pickup";
            pickup.transform.position = position + Vector3.up * 0.5f;
            pickup.transform.localScale = Vector3.one * 0.45f;

            SphereCollider collider = pickup.GetComponent<SphereCollider>();
            collider.isTrigger = true;

            Rigidbody body = pickup.AddComponent<Rigidbody>();
            body.isKinematic = true;
            body.useGravity = false;

            Renderer renderer = pickup.GetComponent<Renderer>();
            renderer.material.color = new Color(0.15f, 1f, 0.35f);

            pickup.AddComponent<ActionRpgPickup>();
        }
    }
}
