<script lang="ts">
  import { onMount } from 'svelte'
  import { Bolt, Play } from '@lucide/svelte'
  import { getAdminClient } from '../client/context.js'
  import type { ActionInputSpec, ActionValue, AppActionDefinition } from '../client/types.js'
  import Button from '../ui/Button.svelte'
  import Field from '../ui/Field.svelte'
  import Input from '../ui/Input.svelte'
  import Panel from '../ui/Panel.svelte'
  const client = getAdminClient()
  const { appActionCatalog, lastOscSendStatus } = client
  let busy = $state(false)
  let values = $state<Record<string, Record<string, string | boolean>>>({})
  let grouped = $derived({
    general: $appActionCatalog.actions.filter(action => action.scope.type === 'kitu-general'),
    project: $appActionCatalog.actions.filter(action => action.scope.type === 'project')
  })
  onMount(() => {
    void client.loadAppActions()
  })
  function defaultValue(input: ActionInputSpec): string | boolean {
    if (input.default?.type === 'bool') return input.default.value
    if (input.default) return String(input.default.value)
    if (input.valueType === 'bool') return false
    if (input.valueType === 'float' || input.valueType === 'int') return '0'
    return ''
  }
  function valueFor(action: AppActionDefinition, input: ActionInputSpec) {
    const actionValues = (values[action.id] ??= {})
    return (actionValues[input.name] ??= defaultValue(input))
  }
  function setValue(action: AppActionDefinition, input: ActionInputSpec, value: string | boolean) {
    values[action.id] = { ...(values[action.id] ?? {}), [input.name]: value }
  }
  async function submit(action: AppActionDefinition) {
    if (busy) return
    const inputs: Record<string, ActionValue> = {}
    for (const input of action.inputs) {
      const raw = valueFor(action, input)
      inputs[input.name] =
        input.valueType === 'bool'
          ? { type: 'bool', value: Boolean(raw) }
          : input.valueType === 'int'
            ? { type: 'int', value: Number(raw) }
            : input.valueType === 'float'
              ? { type: 'float', value: Number(raw) }
              : { type: 'string', value: String(raw) }
    }
    busy = true
    try {
      await client.runAppAction(action.id, inputs)
    } finally {
      busy = false
    }
  }
  const groups = $derived([
    { title: 'Kitu General Actions', eyebrow: 'Kitu general', actions: grouped.general },
    { title: 'Project Actions', eyebrow: 'Project', actions: grouped.project }
  ])
</script>

<div class="grid gap-4">
  <p role="status" aria-live="polite" class="rounded-md border border-border p-3 text-sm">
    {$client.lastOscSendStatus.phase}: {$client.lastOscSendStatus.detail ?? 'Choose an action'}
  </p>
  <div class="grid grid-cols-2 gap-4 max-lg:grid-cols-1">
    {#each groups as group}<Panel title={group.title} eyebrow={group.eyebrow}>
        <div class="grid gap-3">
          {#each group.actions as action}<form
              class="grid gap-3 rounded-md border border-border p-3"
              onsubmit={event => {
                event.preventDefault()
                void submit(action)
              }}>
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <p class="truncate text-sm font-semibold">{action.label}</p>
                  <p class="truncate font-mono text-xs text-muted-foreground">{action.output.address}</p>
                </div>
                <Bolt size={16} class="text-accent" />
              </div>
              <div class="grid gap-2">
                {#each action.inputs as input}<Field label={input.label}>
                    {#if input.valueType === 'bool'}<input
                        type="checkbox"
                        checked={Boolean(valueFor(action, input))}
                        onchange={event => setValue(action, input, event.currentTarget.checked)} />{:else}<Input
                        type={input.valueType === 'string' ? 'text' : 'number'}
                        step={input.valueType === 'int' ? '1' : '0.5'}
                        value={String(valueFor(action, input))}
                        oninput={event => setValue(action, input, event.currentTarget.value)} />{/if}
                  </Field>{/each}
              </div>
              <Button
                disabled={busy}
                type="submit"
                variant={action.ui.destructive ? 'destructive' : group.eyebrow === 'Project' ? 'secondary' : 'default'}>
                <Play size={15} />{action.ui.submitLabel}
              </Button>
            </form>{/each}
        </div>
      </Panel>{/each}
  </div>
</div>
