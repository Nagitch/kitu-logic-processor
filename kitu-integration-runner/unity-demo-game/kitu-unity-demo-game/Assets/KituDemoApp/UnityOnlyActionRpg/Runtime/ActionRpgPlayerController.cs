using System.Collections;
using UnityEngine;
using UnityEngine.InputSystem;

namespace UnityOnlyActionRpg
{
    [RequireComponent(typeof(Rigidbody), typeof(CapsuleCollider))]
    public sealed class ActionRpgPlayerController : MonoBehaviour
    {
        [Header("Movement")]
        [SerializeField, Min(0f)] private float moveSpeed = 6f;
        [SerializeField, Min(0f)] private float turnSpeed = 14f;

        [Header("Combat")]
        [SerializeField, Min(1)] private int maxHealth = 100;
        [SerializeField, Min(1)] private int attackDamage = 25;
        [SerializeField, Min(0.1f)] private float attackRange = 1.6f;
        [SerializeField, Min(0.05f)] private float attackCooldown = 0.4f;
        [SerializeField] private GameObject attackVisual;

        private Rigidbody body;
        private Renderer bodyRenderer;
        private ActionRpgGameController game;
        private Vector2 moveInput;
        private float nextAttackTime;
        private Color normalColor;

        public int CurrentHealth { get; private set; }
        public int MaxHealth => maxHealth;
        public int AttackDamage => attackDamage;

        private void Awake()
        {
            body = GetComponent<Rigidbody>();
            body.constraints = RigidbodyConstraints.FreezeRotationX | RigidbodyConstraints.FreezeRotationZ;
            body.interpolation = RigidbodyInterpolation.Interpolate;

            bodyRenderer = GetComponent<Renderer>();
            if (bodyRenderer != null)
            {
                normalColor = bodyRenderer.material.color;
            }

            if (attackVisual == null)
            {
                attackVisual = transform.Find("Attack Visual")?.gameObject;
            }

            if (attackVisual != null)
            {
                attackVisual.SetActive(false);
            }

            CurrentHealth = maxHealth;
        }

        private void Start()
        {
            game = FindFirstObjectByType<ActionRpgGameController>();
        }

        private void Update()
        {
            if (game == null || !game.IsRunning)
            {
                moveInput = Vector2.zero;
                return;
            }

            ReadMovement();

            Keyboard keyboard = Keyboard.current;
            bool keyboardAttack = keyboard != null &&
                (keyboard.spaceKey.wasPressedThisFrame || keyboard.jKey.wasPressedThisFrame);
            bool mouseAttack = Mouse.current != null && Mouse.current.leftButton.wasPressedThisFrame;
            bool gamepadAttack = Gamepad.current != null && Gamepad.current.buttonSouth.wasPressedThisFrame;

            if (keyboardAttack || mouseAttack || gamepadAttack)
            {
                PerformAttack();
            }

            bool usePotion = keyboard != null && keyboard.eKey.wasPressedThisFrame;
            usePotion |= Gamepad.current != null && Gamepad.current.buttonWest.wasPressedThisFrame;
            if (usePotion && game.TryConsumePotion())
            {
                Heal(40);
            }
        }

        private void FixedUpdate()
        {
            Vector3 direction = new(moveInput.x, 0f, moveInput.y);
            body.MovePosition(body.position + direction * (moveSpeed * Time.fixedDeltaTime));

            if (direction.sqrMagnitude > 0.001f)
            {
                Quaternion targetRotation = Quaternion.LookRotation(direction, Vector3.up);
                body.MoveRotation(Quaternion.Slerp(body.rotation, targetRotation, turnSpeed * Time.fixedDeltaTime));
            }
        }

        public void PerformAttack()
        {
            if (game == null || !game.IsRunning || Time.time < nextAttackTime)
            {
                return;
            }

            nextAttackTime = Time.time + attackCooldown;
            StartCoroutine(ShowAttackVisual());

            Vector3 attackCenter = transform.position + transform.forward * 1.1f;
            Collider[] hits = Physics.OverlapSphere(attackCenter, attackRange, ~0, QueryTriggerInteraction.Ignore);
            foreach (Collider hit in hits)
            {
                ActionRpgEnemy enemy = hit.GetComponentInParent<ActionRpgEnemy>();
                if (enemy != null)
                {
                    enemy.TakeDamage(attackDamage);
                }
            }
        }

        public void TakeDamage(int amount)
        {
            if (game == null || !game.IsRunning || CurrentHealth <= 0)
            {
                return;
            }

            CurrentHealth = Mathf.Max(0, CurrentHealth - amount);
            StartCoroutine(FlashDamage());

            if (CurrentHealth == 0)
            {
                game.RegisterPlayerDeath();
            }
        }

        public void ApplyLevelUp()
        {
            maxHealth += 20;
            attackDamage += 5;
            CurrentHealth = maxHealth;
        }

        public void Heal(int amount)
        {
            CurrentHealth = Mathf.Min(maxHealth, CurrentHealth + amount);
        }

        private void ReadMovement()
        {
            moveInput = Vector2.zero;
            Keyboard keyboard = Keyboard.current;
            if (keyboard != null)
            {
                moveInput.x = (keyboard.dKey.isPressed || keyboard.rightArrowKey.isPressed ? 1f : 0f) -
                    (keyboard.aKey.isPressed || keyboard.leftArrowKey.isPressed ? 1f : 0f);
                moveInput.y = (keyboard.wKey.isPressed || keyboard.upArrowKey.isPressed ? 1f : 0f) -
                    (keyboard.sKey.isPressed || keyboard.downArrowKey.isPressed ? 1f : 0f);
            }

            if (Gamepad.current != null && Gamepad.current.leftStick.ReadValue().sqrMagnitude > moveInput.sqrMagnitude)
            {
                moveInput = Gamepad.current.leftStick.ReadValue();
            }

            moveInput = Vector2.ClampMagnitude(moveInput, 1f);
        }

        private IEnumerator ShowAttackVisual()
        {
            if (attackVisual == null)
            {
                yield break;
            }

            attackVisual.SetActive(true);
            yield return new WaitForSeconds(0.12f);
            attackVisual.SetActive(false);
        }

        private IEnumerator FlashDamage()
        {
            if (bodyRenderer == null)
            {
                yield break;
            }

            bodyRenderer.material.color = Color.red;
            yield return new WaitForSeconds(0.12f);
            bodyRenderer.material.color = normalColor;
        }

        private void OnDrawGizmosSelected()
        {
            Gizmos.color = Color.yellow;
            Gizmos.DrawWireSphere(transform.position + transform.forward * 1.1f, attackRange);
        }
    }
}
