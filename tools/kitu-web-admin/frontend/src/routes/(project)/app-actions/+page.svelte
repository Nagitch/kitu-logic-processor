<script lang="ts">
  import { onMount } from "svelte";
  import { Bolt, Play } from "@lucide/svelte";
  import {
    appActionCatalog,
    loadAppActions,
    runAppAction,
  } from "$lib/admin-client";
  import Button from "$lib/components/ui/Button.svelte";
  import Field from "$lib/components/ui/Field.svelte";
  import Input from "$lib/components/ui/Input.svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
  import type {
    ActionInputSpec,
    ActionValue,
    AppActionDefinition,
  } from "$lib/types";

  let inputValues: Record<string, Record<string, string | boolean>> = {};

  $: groupedActions = {
    general: $appActionCatalog.actions.filter(
      (action) => action.scope.type === "kitu-general",
    ),
    project: $appActionCatalog.actions.filter(
      (action) => action.scope.type === "project",
    ),
  };

  onMount(() => {
    loadAppActions();
  });

  function valueFor(action: AppActionDefinition, input: ActionInputSpec) {
    const actionValues = (inputValues[action.id] ??= {});
    if (actionValues[input.name] === undefined) {
      actionValues[input.name] = defaultInputValue(input);
    }
    return actionValues[input.name];
  }

  function defaultInputValue(input: ActionInputSpec): string | boolean {
    if (input.default?.type === "bool") return input.default.value;
    if (input.default) return String(input.default.value);
    if (input.valueType === "bool") return false;
    if (input.valueType === "float" || input.valueType === "int") return "0";
    return "";
  }

  function setValue(
    action: AppActionDefinition,
    input: ActionInputSpec,
    value: string | boolean,
  ) {
    inputValues[action.id] = {
      ...(inputValues[action.id] ?? {}),
      [input.name]: value,
    };
  }

  function submitAction(action: AppActionDefinition) {
    const inputs: Record<string, ActionValue> = {};
    for (const input of action.inputs) {
      const raw = valueFor(action, input);
      if (input.valueType === "bool") {
        inputs[input.name] = { type: "bool", value: Boolean(raw) };
      } else if (input.valueType === "int") {
        inputs[input.name] = { type: "int", value: Number(raw) };
      } else if (input.valueType === "float") {
        inputs[input.name] = { type: "float", value: Number(raw) };
      } else {
        inputs[input.name] = { type: "string", value: String(raw) };
      }
    }
    runAppAction(action.id, inputs);
  }
</script>

<svelte:head>
  <title>App Actions - Kitu Admin</title>
</svelte:head>

<div class="grid gap-4">
  <div class="grid grid-cols-2 gap-4 max-lg:grid-cols-1">
    <Panel title="Kitu General Actions" eyebrow="Kitu general">
      <div class="grid gap-3">
        {#each groupedActions.general as action}
          <form
            class="grid gap-3 rounded-md border border-border p-3"
            onsubmit={(event) => {
              event.preventDefault();
              submitAction(action);
            }}
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <p class="truncate text-sm font-semibold">{action.label}</p>
                <p class="truncate font-mono text-xs text-muted-foreground">
                  {action.output.address}
                </p>
              </div>
              <Bolt size={16} class="text-accent" />
            </div>
            <div class="grid gap-2">
              {#each action.inputs as input}
                <Field label={input.label}>
                  {#if input.valueType === "bool"}
                    <input
                      type="checkbox"
                      checked={Boolean(valueFor(action, input))}
                      onchange={(event) =>
                        setValue(action, input, event.currentTarget.checked)}
                    />
                  {:else}
                    <Input
                      type={input.valueType === "string" ? "text" : "number"}
                      step={input.valueType === "int" ? "1" : "0.5"}
                      value={String(valueFor(action, input))}
                      oninput={(event) =>
                        setValue(action, input, event.currentTarget.value)}
                    />
                  {/if}
                </Field>
              {/each}
            </div>
            <Button
              type="submit"
              variant={action.ui.destructive ? "destructive" : "default"}
            >
              <Play size={15} />
              {action.ui.submitLabel}
            </Button>
          </form>
        {/each}
      </div>
    </Panel>

    <Panel title="Project Actions" eyebrow="Project">
      <div class="grid gap-3">
        {#each groupedActions.project as action}
          <form
            class="grid gap-3 rounded-md border border-border p-3"
            onsubmit={(event) => {
              event.preventDefault();
              submitAction(action);
            }}
          >
            <div class="min-w-0">
              <p class="truncate text-sm font-semibold">{action.label}</p>
              <p class="truncate font-mono text-xs text-muted-foreground">
                {action.id} · {action.output.address}
              </p>
            </div>
            <div class="grid gap-2">
              {#each action.inputs as input}
                <Field label={input.label}>
                  {#if input.valueType === "bool"}
                    <input
                      type="checkbox"
                      checked={Boolean(valueFor(action, input))}
                      onchange={(event) =>
                        setValue(action, input, event.currentTarget.checked)}
                    />
                  {:else}
                    <Input
                      type={input.valueType === "string" ? "text" : "number"}
                      step={input.valueType === "int" ? "1" : "0.5"}
                      value={String(valueFor(action, input))}
                      oninput={(event) =>
                        setValue(action, input, event.currentTarget.value)}
                    />
                  {/if}
                </Field>
              {/each}
            </div>
            <Button type="submit" variant="secondary">
              <Play size={15} />
              {action.ui.submitLabel}
            </Button>
          </form>
        {/each}
      </div>
    </Panel>
  </div>
</div>
