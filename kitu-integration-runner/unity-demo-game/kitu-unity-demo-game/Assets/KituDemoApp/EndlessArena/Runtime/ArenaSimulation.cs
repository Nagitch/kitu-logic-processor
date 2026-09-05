using System;
using System.Collections.Generic;
using UnityEngine;

namespace UnityOnlyArena
{
    public enum ArenaPhase { Opening, Preparing, Transition, Combat, Cleared, Results }
    public enum ArenaEnemyKind { Pursuer, Shooter, Heavy, Boss }
    public enum ArenaBossState { Pursuit, Telegraph, Recovery }
    public enum ArenaEffectKind { Slash, Explosion, Hit }

    public struct ArenaInput
    {
        public Vector2 Move;
        public Vector2 AimPoint;
        public bool HasAim;
        public bool FireA;
        public bool FireB;
        public bool UseA;
        public bool UseB;
    }

    public sealed class ArenaEnemy
    {
        public int Id;
        public ArenaEnemyKind Kind;
        public Vector2 Position;
        public int Health;
        public int MaxHealth;
        public float Radius;
        public float Speed;
        public int Damage;
        public float AttackInterval;
        public float AttackRange;
        public float AttackCooldown;
        public ArenaBossState BossState;
        public float PhaseRemaining;
    }

    public sealed class ArenaProjectile
    {
        public int Id;
        public Vector2 Position;
        public Vector2 Direction;
        public float Speed;
        public float DistanceRemaining;
        public int Damage;
        public bool EnemyOwned;
        public float Radius = 0.14f;
    }

    public sealed class ArenaGrenade
    {
        public int Id;
        public Vector2 Start;
        public Vector2 Target;
        public Vector2 Position;
        public float Remaining;
        public int Damage;
    }

    public sealed class ArenaEffect
    {
        public int Id;
        public ArenaEffectKind Kind;
        public Vector2 Position;
        public Vector2 Direction;
        public float Radius;
        public float Remaining;
    }

    // Copies values at death; result rendering never reads subsequently mutable inventory.
    public sealed class ArenaResult
    {
        public int Floor { get; private set; }
        public float Elapsed { get; private set; }
        public int EnemiesDefeated { get; private set; }
        public int BossesDefeated { get; private set; }
        public int FloorsCleared { get; private set; }
        public int MaxHealth { get; private set; }
        public float AttackMultiplier { get; private set; }
        public IReadOnlyList<string> EquipmentNames { get; private set; }

        internal ArenaResult(ArenaSimulation game)
        {
            Floor = game.Floor;
            Elapsed = game.Elapsed;
            EnemiesDefeated = game.EnemiesDefeated;
            BossesDefeated = game.BossesDefeated;
            FloorsCleared = game.FloorsCleared;
            MaxHealth = game.Inventory.MaxHealth;
            AttackMultiplier = game.Inventory.AttackMultiplier;
            var names = new string[4];
            for (int i = 0; i < names.Length; i++)
                names[i] = game.Inventory.Equipment[i] == null ? "Empty" : game.Inventory.Equipment[i].Name;
            EquipmentNames = Array.AsReadOnly(names);
        }
    }

    /// <summary>
    /// Unity-only rules/state for one run. A view calls Step at 1/60 second only while gameplay
    /// is active; pausing the view therefore freezes every timer and input in the same place.
    /// Vector2.y maps to world Z. No scene object, physics callback or animation owns a rule.
    /// </summary>
    public sealed class ArenaSimulation
    {
        public const float ArenaHalfExtent = 10f;
        public const float PlayerSpeed = 5f;
        public const float PlayerRadius = 0.5f;
        public const float PortalRadius = 1.25f;
        public const float TransitionDuration = 0.3f;
        public const float GrenadeFlightTime = 0.5f;
        public const float GrenadeRadius = 3f;
        public static readonly Vector2 EntrancePosition = new Vector2(0f, -7f);
        public static readonly Vector2 PortalPosition = new Vector2(0f, 7.5f);
        public static readonly Vector2 ChestPosition = new Vector2(-3f, 0f);

        private static readonly Vector2[] SpawnPositions =
        {
            new Vector2(-7.5f, 6f), new Vector2(-4.5f, 6f), new Vector2(-1.5f, 6f),
            new Vector2(1.5f, 6f), new Vector2(4.5f, 6f), new Vector2(7.5f, 6f),
            new Vector2(-7.5f, 2.5f), new Vector2(-4.5f, 2.5f), new Vector2(-1.5f, 2.5f),
            new Vector2(1.5f, 2.5f), new Vector2(4.5f, 2.5f), new Vector2(7.5f, 2.5f)
        };

        public ArenaPhase Phase { get; private set; } = ArenaPhase.Opening;
        public ArenaInventory Inventory { get; private set; } = new ArenaInventory();
        public int Floor { get; private set; }
        public float Elapsed { get; private set; }
        public int EnemiesDefeated { get; private set; }
        public int BossesDefeated { get; private set; }
        public int FloorsCleared { get; private set; }
        public Vector2 PlayerPosition;
        public Vector2 AimDirection = Vector2.up;
        public readonly List<ArenaEnemy> Enemies = new List<ArenaEnemy>();
        public readonly List<ArenaProjectile> Projectiles = new List<ArenaProjectile>();
        public readonly List<ArenaGrenade> Grenades = new List<ArenaGrenade>();
        public readonly List<ArenaEffect> Effects = new List<ArenaEffect>();
        public readonly float[] WeaponCooldowns = new float[2];
        public ArenaResult Result { get; private set; }
        public string LastMessage { get; private set; } = "";
        public bool PortalAvailable { get { return Phase == ArenaPhase.Preparing || Phase == ArenaPhase.Cleared; } }
        public bool ChestAvailable { get { return Phase == ArenaPhase.Preparing || (Phase == ArenaPhase.Cleared && Floor % 5 == 0); } }
        public bool IsSafe { get { return Phase == ArenaPhase.Preparing || Phase == ArenaPhase.Cleared; } }
        public bool IsBossFloor { get { return Floor > 0 && Floor % 5 == 0; } }

        private readonly Dictionary<ArenaEnemy, int> pendingEnemyDamage = new Dictionary<ArenaEnemy, int>();
        private readonly List<int> pendingPlayerDamage = new List<int>();
        private int nextId = 1;
        private float transitionRemaining;
        private bool portalArmed;

        public void StartRun()
        {
            Inventory.Reset();
            Inventory.CreateChest(0);
            Floor = 0;
            Elapsed = 0f;
            EnemiesDefeated = 0;
            BossesDefeated = 0;
            FloorsCleared = 0;
            Result = null;
            nextId = 1;
            Enemies.Clear();
            ClearTransientState();
            ResetPlayerEntry();
            Phase = ArenaPhase.Preparing;
            portalArmed = true;
            LastMessage = "Choose equipment from the chest, then enter the portal.";
        }

        public void ReturnToMenu()
        {
            Enemies.Clear();
            ClearTransientState();
            Inventory.Reset();
            Array.Clear(Inventory.Backpack, 0, Inventory.Backpack.Length);
            Array.Clear(Inventory.Equipment, 0, Inventory.Equipment.Length);
            Inventory.Chest.Clear();
            Result = null;
            Floor = 0;
            Elapsed = 0f;
            EnemiesDefeated = BossesDefeated = FloorsCleared = 0;
            ResetPlayerEntry();
            Phase = ArenaPhase.Opening;
            portalArmed = false;
            LastMessage = "";
        }

        public bool TryAdvanceFloor()
        {
            if (!PortalAvailable) return false;
            bool hasWeapon = false;
            for (int i = 0; i < 2; i++)
                hasWeapon |= Inventory.Equipment[i] != null && Inventory.Equipment[i].IsWeapon;
            if (!hasWeapon)
            {
                LastMessage = "Equip at least one weapon before entering the next floor.";
                return false;
            }

            Phase = ArenaPhase.Transition;
            transitionRemaining = TransitionDuration;
            portalArmed = false;
            Inventory.Chest.Clear();
            Enemies.Clear();
            ClearTransientState();
            return true;
        }

        public void Step(float dt, ArenaInput input)
        {
            if (dt <= 0f || float.IsNaN(dt) || float.IsInfinity(dt)) return;
            if (Phase == ArenaPhase.Opening || Phase == ArenaPhase.Results) return;
            if (Phase == ArenaPhase.Transition)
            {
                transitionRemaining -= dt;
                if (transitionRemaining <= 0.00001f) EnterNextFloor();
                return;
            }

            Elapsed += dt;
            pendingEnemyDamage.Clear();
            pendingPlayerDamage.Clear();
            AgeEffects(dt);
            for (int i = 0; i < 2; i++) WeaponCooldowns[i] = Mathf.Max(0f, WeaponCooldowns[i] - dt);
            if (IsFinite(input.Move))
                PlayerPosition = ClampToArena(PlayerPosition + Vector2.ClampMagnitude(input.Move, 1f) * (PlayerSpeed * dt), PlayerRadius);
            if (input.HasAim && IsFinite(input.AimPoint))
            {
                Vector2 direction = input.AimPoint - PlayerPosition;
                if (direction.sqrMagnitude > 0.000001f) AimDirection = direction.normalized;
            }

            // Consumables precede this update's attacks/damage. A heals before B is checked.
            if (input.UseA) UseItem(EquipmentSlot.ItemA, input);
            if (input.UseB) UseItem(EquipmentSlot.ItemB, input);
            if (input.FireA) UseWeapon(0);
            if (input.FireB) UseWeapon(1);
            if (Phase == ArenaPhase.Combat)
                for (int i = 0; i < Enemies.Count; i++) UpdateEnemy(Enemies[i], dt);

            UpdateProjectiles(dt);
            UpdateGrenades(dt);
            ResolveDamage();
            Inventory.AdvanceTime(dt);

            // Death wins even when the last enemy/boss also died in this update.
            if (Inventory.Health <= 0)
            {
                Phase = ArenaPhase.Results;
                Result = new ArenaResult(this);
                ClearTransientState();
                LastMessage = "Game over.";
                return;
            }
            if (Phase == ArenaPhase.Combat && Enemies.Count == 0) ClearFloor();
            UpdatePortalOverlap();
        }

        public static Vector2 ClampToArena(Vector2 point, float radius = 0f)
        {
            float edge = ArenaHalfExtent - radius;
            return new Vector2(Mathf.Clamp(point.x, -edge, edge), Mathf.Clamp(point.y, -edge, edge));
        }

        private void ResetPlayerEntry()
        {
            PlayerPosition = EntrancePosition;
            AimDirection = Vector2.up;
            WeaponCooldowns[0] = WeaponCooldowns[1] = 0f;
        }

        private void EnterNextFloor()
        {
            Floor++;
            ResetPlayerEntry();
            Enemies.Clear();
            if (IsBossFloor)
                Enemies.Add(CreateEnemy(ArenaEnemyKind.Boss, new Vector2(0f, 5.5f)));
            else
            {
                int count = Mathf.Min(Floor + 2, 12);
                int shooters = Floor == 1 ? 0 : count / 3;
                int heavy = Floor <= 2 ? 0 : count / 4;
                int pursuers = count - shooters - heavy;
                for (int i = 0; i < count; i++)
                {
                    ArenaEnemyKind kind = i < pursuers ? ArenaEnemyKind.Pursuer
                        : i < pursuers + shooters ? ArenaEnemyKind.Shooter : ArenaEnemyKind.Heavy;
                    Enemies.Add(CreateEnemy(kind, SpawnPositions[i]));
                }
            }
            // Publish Combat only after the complete roster exists.
            Phase = ArenaPhase.Combat;
            LastMessage = IsBossFloor ? "Defeat the boss." : "Defeat every enemy.";
        }

        private ArenaEnemy CreateEnemy(ArenaEnemyKind kind, Vector2 position)
        {
            var enemy = new ArenaEnemy { Id = nextId++, Kind = kind, Position = position, Radius = 0.5f };
            int baseHealth;
            int baseDamage;
            switch (kind)
            {
                case ArenaEnemyKind.Shooter:
                    baseHealth = 30; baseDamage = 8; enemy.Speed = 2f; enemy.AttackRange = 6f; enemy.AttackInterval = 1.5f;
                    break;
                case ArenaEnemyKind.Heavy:
                    baseHealth = 100; baseDamage = 20; enemy.Speed = 1.2f; enemy.AttackRange = 1.8f; enemy.AttackInterval = 1.5f; enemy.Radius = 0.7f;
                    break;
                case ArenaEnemyKind.Boss:
                    baseHealth = 300; baseDamage = 20; enemy.Speed = 1.5f; enemy.AttackRange = 2f; enemy.AttackInterval = 1.5f; enemy.Radius = 1f;
                    enemy.BossState = ArenaBossState.Pursuit; enemy.PhaseRemaining = 3f;
                    break;
                default:
                    baseHealth = 40; baseDamage = 10; enemy.Speed = 2.5f; enemy.AttackRange = 1.5f; enemy.AttackInterval = 1f;
                    break;
            }
            enemy.MaxHealth = Mathf.CeilToInt(baseHealth * (1f + 0.12f * (Floor - 1)));
            enemy.Health = enemy.MaxHealth;
            enemy.Damage = ScaleEnemyDamage(baseDamage);
            enemy.AttackCooldown = enemy.AttackInterval;
            return enemy;
        }

        private int ScaleEnemyDamage(int damage)
        {
            return Mathf.Max(1, Mathf.FloorToInt(damage * (1f + 0.08f * (Floor - 1))));
        }

        private int ScalePlayerDamage(int damage)
        {
            return Mathf.Max(1, Mathf.FloorToInt(damage * Inventory.AttackMultiplier));
        }

        private void UseWeapon(int slot)
        {
            var weapon = Inventory.Equipment[slot];
            if (weapon == null || !weapon.IsWeapon || WeaponCooldowns[slot] > 0.00001f) return;
            WeaponCooldowns[slot] = weapon.Interval;
            int damage = ScalePlayerDamage(weapon.Damage);
            if (weapon.Kind == ItemKind.Blade)
            {
                AddEffect(ArenaEffectKind.Slash, PlayerPosition, AimDirection, 2f, 0.15f);
                for (int i = 0; i < Enemies.Count; i++)
                {
                    var enemy = Enemies[i];
                    Vector2 offset = enemy.Position - PlayerPosition;
                    if (enemy.Health > 0 && offset.magnitude <= 2f + enemy.Radius &&
                        (offset.sqrMagnitude < 0.000001f || Vector2.Dot(offset.normalized, AimDirection) >= 0.7071067f))
                        QueueEnemyDamage(enemy, damage);
                }
            }
            else
            {
                bool heavy = weapon.Kind == ItemKind.HeavyShooter;
                SpawnProjectile(PlayerPosition, AimDirection, heavy ? 7f : 12f, heavy ? 10f : 12f, damage, false);
            }
        }

        private void UseItem(EquipmentSlot slot, ArenaInput input)
        {
            var equipped = Inventory.Equipment[(int)slot];
            if (equipped == null) return;
            Vector2 target = PlayerPosition;
            if (equipped.Kind == ItemKind.Grenade)
            {
                if (!input.HasAim || !IsFinite(input.AimPoint) || (input.AimPoint - PlayerPosition).sqrMagnitude <= 0.000001f)
                {
                    LastMessage = "Aim at a floor position before throwing a grenade.";
                    return;
                }
                target = ClampToArena(PlayerPosition + Vector2.ClampMagnitude(input.AimPoint - PlayerPosition, 8f), 0.1f);
            }
            ArenaItem used;
            bool accepted = Inventory.UseItem(slot, out used);
            LastMessage = Inventory.LastMessage;
            if (!accepted) return;
            if (used.Kind == ItemKind.Grenade)
            {
                Grenades.Add(new ArenaGrenade
                {
                    Id = nextId++, Start = PlayerPosition, Position = PlayerPosition, Target = target,
                    Remaining = GrenadeFlightTime, Damage = ScalePlayerDamage(100)
                });
            }
        }

        private void UpdateEnemy(ArenaEnemy enemy, float dt)
        {
            if (enemy.Health <= 0) return;
            enemy.AttackCooldown = Mathf.Max(0f, enemy.AttackCooldown - dt);
            if (enemy.Kind == ArenaEnemyKind.Boss)
            {
                enemy.PhaseRemaining -= dt;
                if (enemy.BossState == ArenaBossState.Pursuit && enemy.PhaseRemaining <= 0.00001f)
                {
                    enemy.BossState = ArenaBossState.Telegraph;
                    enemy.PhaseRemaining = 0.8f;
                    return;
                }
                if (enemy.BossState == ArenaBossState.Telegraph)
                {
                    if (enemy.PhaseRemaining <= 0.00001f)
                    {
                        for (int i = 0; i < 8; i++)
                        {
                            float angle = i * Mathf.PI / 4f;
                            SpawnProjectile(enemy.Position, new Vector2(Mathf.Cos(angle), Mathf.Sin(angle)), 5f, 20f, ScaleEnemyDamage(12), true);
                        }
                        enemy.BossState = ArenaBossState.Recovery;
                        enemy.PhaseRemaining = 1f;
                    }
                    return;
                }
                if (enemy.BossState == ArenaBossState.Recovery)
                {
                    if (enemy.PhaseRemaining <= 0.00001f)
                    {
                        enemy.BossState = ArenaBossState.Pursuit;
                        enemy.PhaseRemaining = 3f;
                    }
                    return;
                }
            }

            Vector2 offset = PlayerPosition - enemy.Position;
            float distance = offset.magnitude;
            if (distance > enemy.AttackRange + 0.0001f)
            {
                float travel = Mathf.Min(enemy.Speed * dt, distance - enemy.AttackRange);
                enemy.Position = ClampToArena(enemy.Position + offset.normalized * travel, enemy.Radius);
            }
            else if (enemy.AttackCooldown <= 0.00001f)
            {
                enemy.AttackCooldown = enemy.AttackInterval;
                if (enemy.Kind == ArenaEnemyKind.Shooter)
                    SpawnProjectile(enemy.Position, distance <= 0.00001f ? Vector2.up : offset.normalized, 6f, 12f, enemy.Damage, true);
                else
                {
                    pendingPlayerDamage.Add(enemy.Damage);
                    AddEffect(ArenaEffectKind.Hit, PlayerPosition, offset.normalized, PlayerRadius, 0.15f);
                }
            }
        }

        private void SpawnProjectile(Vector2 position, Vector2 direction, float speed, float distance, int damage, bool enemyOwned)
        {
            Projectiles.Add(new ArenaProjectile
            {
                Id = nextId++, Position = position, Direction = direction, Speed = speed,
                DistanceRemaining = distance, Damage = damage, EnemyOwned = enemyOwned
            });
        }

        private void UpdateProjectiles(float dt)
        {
            for (int i = Projectiles.Count - 1; i >= 0; i--)
            {
                var shot = Projectiles[i];
                Vector2 from = shot.Position;
                float travel = Mathf.Min(shot.Speed * dt, shot.DistanceRemaining);
                float wallDistance = DistanceToWall(from, shot.Direction, shot.Radius);
                bool hitsWall = wallDistance <= travel;
                travel = Mathf.Min(travel, wallDistance);
                Vector2 to = from + shot.Direction * travel;
                ArenaEnemy closestEnemy = null;
                float closest = float.PositiveInfinity;
                if (shot.EnemyOwned)
                {
                    float hit;
                    if (Phase == ArenaPhase.Combat && SegmentCircle(from, to, PlayerPosition, PlayerRadius + shot.Radius, out hit))
                        closest = hit;
                }
                else
                {
                    for (int j = 0; j < Enemies.Count; j++)
                    {
                        var enemy = Enemies[j];
                        float hit;
                        if (enemy.Health > 0 && SegmentCircle(from, to, enemy.Position, enemy.Radius + shot.Radius, out hit) && hit < closest)
                        {
                            closest = hit;
                            closestEnemy = enemy;
                        }
                    }
                }
                if (!float.IsPositiveInfinity(closest))
                {
                    Vector2 impact = Vector2.Lerp(from, to, closest);
                    if (shot.EnemyOwned) pendingPlayerDamage.Add(shot.Damage);
                    else QueueEnemyDamage(closestEnemy, shot.Damage);
                    AddEffect(ArenaEffectKind.Hit, impact, shot.Direction, 0.35f, 0.12f);
                    Projectiles.RemoveAt(i);
                    continue;
                }
                shot.Position = to;
                shot.DistanceRemaining -= travel;
                if (hitsWall || shot.DistanceRemaining <= 0.00001f) Projectiles.RemoveAt(i);
            }
        }

        private static float DistanceToWall(Vector2 position, Vector2 direction, float radius)
        {
            float edge = ArenaHalfExtent - radius;
            float distance = float.PositiveInfinity;
            if (direction.x > 0f) distance = Mathf.Min(distance, (edge - position.x) / direction.x);
            if (direction.x < 0f) distance = Mathf.Min(distance, (-edge - position.x) / direction.x);
            if (direction.y > 0f) distance = Mathf.Min(distance, (edge - position.y) / direction.y);
            if (direction.y < 0f) distance = Mathf.Min(distance, (-edge - position.y) / direction.y);
            return Mathf.Max(0f, distance);
        }

        private static bool SegmentCircle(Vector2 start, Vector2 end, Vector2 center, float radius, out float hit)
        {
            Vector2 relative = start - center;
            float c = relative.sqrMagnitude - radius * radius;
            if (c <= 0f) { hit = 0f; return true; }
            Vector2 segment = end - start;
            float a = segment.sqrMagnitude;
            if (a <= 0.0000001f) { hit = 0f; return false; }
            float b = Vector2.Dot(relative, segment);
            float discriminant = b * b - a * c;
            if (discriminant < 0f) { hit = 0f; return false; }
            hit = (-b - Mathf.Sqrt(discriminant)) / a;
            return hit >= 0f && hit <= 1f;
        }

        private void UpdateGrenades(float dt)
        {
            for (int i = Grenades.Count - 1; i >= 0; i--)
            {
                var grenade = Grenades[i];
                grenade.Remaining -= dt;
                grenade.Position = Vector2.Lerp(grenade.Start, grenade.Target, Mathf.Clamp01(1f - grenade.Remaining / GrenadeFlightTime));
                if (grenade.Remaining > 0.00001f) continue;
                for (int j = 0; j < Enemies.Count; j++)
                {
                    var enemy = Enemies[j];
                    if (enemy.Health > 0 && Vector2.Distance(grenade.Target, enemy.Position) <= GrenadeRadius + enemy.Radius)
                        QueueEnemyDamage(enemy, grenade.Damage);
                }
                AddEffect(ArenaEffectKind.Explosion, grenade.Target, Vector2.up, GrenadeRadius, 0.3f);
                Grenades.RemoveAt(i);
            }
        }

        private void QueueEnemyDamage(ArenaEnemy enemy, int damage)
        {
            int previous;
            pendingEnemyDamage.TryGetValue(enemy, out previous);
            pendingEnemyDamage[enemy] = previous + damage;
        }

        private void ResolveDamage()
        {
            foreach (var entry in pendingEnemyDamage)
                entry.Key.Health = Mathf.Max(0, entry.Key.Health - entry.Value);
            for (int i = Enemies.Count - 1; i >= 0; i--)
            {
                if (Enemies[i].Health > 0) continue;
                EnemiesDefeated++;
                if (Enemies[i].Kind == ArenaEnemyKind.Boss) BossesDefeated++;
                Enemies.RemoveAt(i);
            }
            for (int i = 0; i < pendingPlayerDamage.Count; i++) Inventory.ApplyDamage(pendingPlayerDamage[i]);
            pendingEnemyDamage.Clear();
            pendingPlayerDamage.Clear();
        }

        private void ClearFloor()
        {
            Phase = ArenaPhase.Cleared;
            FloorsCleared++;
            ClearTransientState();
            if (IsBossFloor)
            {
                Inventory.HealFully();
                Inventory.CreateChest(Floor);
            }
            portalArmed = Vector2.Distance(PlayerPosition, PortalPosition) > PortalRadius;
            LastMessage = IsBossFloor ? "Boss defeated. HP restored; choose a reward, then use the portal." : "Floor cleared. Enter the portal to continue.";
        }

        private void UpdatePortalOverlap()
        {
            if (!PortalAvailable) return;
            bool overlaps = Vector2.Distance(PlayerPosition, PortalPosition) <= PortalRadius;
            if (!overlaps) portalArmed = true;
            else if (portalArmed) TryAdvanceFloor();
        }

        private void ClearTransientState()
        {
            Projectiles.Clear();
            Grenades.Clear();
            Effects.Clear();
            pendingEnemyDamage.Clear();
            pendingPlayerDamage.Clear();
        }

        private void AddEffect(ArenaEffectKind kind, Vector2 position, Vector2 direction, float radius, float duration)
        {
            Effects.Add(new ArenaEffect { Id = nextId++, Kind = kind, Position = position, Direction = direction, Radius = radius, Remaining = duration });
        }

        private void AgeEffects(float dt)
        {
            for (int i = Effects.Count - 1; i >= 0; i--)
            {
                Effects[i].Remaining -= dt;
                if (Effects[i].Remaining <= 0f) Effects.RemoveAt(i);
            }
        }

        private static bool IsFinite(Vector2 point)
        {
            return !float.IsNaN(point.x) && !float.IsInfinity(point.x) && !float.IsNaN(point.y) && !float.IsInfinity(point.y);
        }
    }
}
