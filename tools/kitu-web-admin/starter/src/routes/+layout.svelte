<script lang="ts">
  import '../app.css'
  import { base } from '$app/paths'
  import { page } from '$app/stores'
  import { Activity, Bolt, Boxes, ScrollText, Terminal } from '@lucide/svelte'
  import { createAdminClient, setAdminClientContext } from '@kitu/admin/client'
  import { AdminShell, type AdminNavSection } from '@kitu/admin/ui'
  import { onMount } from 'svelte'
  import { adminOptions } from '$lib/admin'
  import { createStarterOscIrLoader } from '$lib/osc-ir'
  let { children }: { children?: import('svelte').Snippet } = $props()
  const admin = createAdminClient(adminOptions)
  setAdminClientContext(admin)
  onMount(() => {
    void createStarterOscIrLoader()
      .initialize()
      .catch(error => console.warn('Kitu OSC-IR WASM smoke initialization failed', error))
  })
  const sections: AdminNavSection[] = [
    {
      id: 'kitu',
      label: 'Kitu',
      items: [
        { href: '/', label: 'Overview', icon: Activity },
        { href: '/world', label: 'World', icon: Boxes },
        { href: '/logs', label: 'Logs', icon: ScrollText },
        { href: '/shell', label: 'Shell', icon: Terminal },
        { href: '/app-actions', label: 'App Actions', icon: Bolt }
      ]
    }
  ]
</script>

<AdminShell
  client={admin}
  currentPath={$page.url.pathname}
  {sections}
  brand="Kitu Admin Starter"
  subtitle="Sample application"
  basePath={base}>
  {@render children?.()}
</AdminShell>
