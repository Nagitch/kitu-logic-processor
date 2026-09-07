using System.Collections;
using UnityEngine;

namespace UnityOnlyActionRpg
{
    public sealed class ActionRpgEnemy : MonoBehaviour
    {
        [SerializeField, Min(1)] private int maxHealth = 50;
        [SerializeField, Min(0f)] private float moveSpeed = 2.2f;
        [SerializeField, Min(0.1f)] private float attackRange = 1.35f;
        [SerializeField, Min(1)] private int attackDamage = 12;
        [SerializeField, Min(0.1f)] private float attackCooldown = 1.1f;

        private ActionRpgPlayerController player;
        private ActionRpgGameController game;
        private Renderer bodyRenderer;
        private Color normalColor;
        private int currentHealth;
        private float nextAttackTime;
        private bool dead;

        private void Awake()
        {
            bodyRenderer = GetComponentInChildren<Renderer>();
            if (bodyRenderer != null)
            {
                normalColor = bodyRenderer.material.color;
            }
        }

        private void OnEnable()
        {
            currentHealth = maxHealth;
            dead = false;
        }

        private void Start()
        {
            player = FindFirstObjectByType<ActionRpgPlayerController>();
            game = FindFirstObjectByType<ActionRpgGameController>();
        }

        private void Update()
        {
            if (dead || player == null || game == null || !game.IsRunning)
            {
                return;
            }

            Vector3 offset = player.transform.position - transform.position;
            offset.y = 0f;
            float distance = offset.magnitude;

            if (distance > attackRange)
            {
                Vector3 direction = offset.normalized;
                transform.position += direction * (moveSpeed * Time.deltaTime);
                transform.rotation = Quaternion.Slerp(
                    transform.rotation,
                    Quaternion.LookRotation(direction, Vector3.up),
                    10f * Time.deltaTime);
            }
            else if (Time.time >= nextAttackTime)
            {
                nextAttackTime = Time.time + attackCooldown;
                player.TakeDamage(attackDamage);
            }
        }

        public void TakeDamage(int amount)
        {
            if (dead || game == null || !game.IsRunning)
            {
                return;
            }

            currentHealth -= amount;
            StartCoroutine(FlashHit());

            if (currentHealth <= 0)
            {
                Die();
            }
        }

        private void Die()
        {
            dead = true;
            game.RegisterEnemyDefeated(transform.position);
            Destroy(gameObject);
        }

        private IEnumerator FlashHit()
        {
            if (bodyRenderer == null)
            {
                yield break;
            }

            bodyRenderer.material.color = Color.white;
            yield return new WaitForSeconds(0.08f);
            if (bodyRenderer != null)
            {
                bodyRenderer.material.color = normalColor;
            }
        }
    }
}
