<script lang="ts">
  import "../app.css";
  import { base } from "$app/paths";
  import { page } from "$app/stores";
  import {
    Activity,
    Boxes,
    Bolt,
    BookOpen,
    FileCode,
    Map,
    Play,
    ScrollText,
    SlidersHorizontal,
    Terminal,
  } from "@lucide/svelte";
  import { createAdminClient, setAdminClientContext } from "@kitu/admin/client";
  import {
    AdminShell,
    isAdminHrefActive,
    type AdminNavSection,
  } from "@kitu/admin/ui";
  import { adminClientOptions } from "$lib/admin-client";
  let { children }: { children?: import("svelte").Snippet } = $props();
  const adminClient = createAdminClient(adminClientOptions);
  setAdminClientContext(adminClient);
  const sections: AdminNavSection[] = [
    {
      id: "kitu-general",
      label: "Kitu general",
      items: [
        { href: "/", label: "Overview", icon: Activity },
        { href: "/world", label: "World", icon: Boxes },
        { href: "/logs", label: "Logs", icon: ScrollText },
        { href: "/shell", label: "Shell", icon: Terminal },
      ],
    },
    {
      id: "project",
      label: "Project",
      items: [
        { href: "/app-actions", label: "App Actions", icon: Bolt },
        { href: "/arena-inspector", label: "Arena Inspector", icon: Activity },
        { href: "/arena-replay", label: "Arena Replay", icon: Play },
        { href: "/level-designer", label: "Level Designer", icon: Map },
        {
          href: "/story-sequencing",
          label: "Story Sequencing",
          icon: BookOpen,
        },
        {
          href: "/game-parameters",
          label: "Game Parameters",
          icon: SlidersHorizontal,
        },
        { href: "/game-scripts", label: "Game Scripts", icon: FileCode },
      ],
    },
  ];
</script>

<AdminShell
  client={adminClient}
  currentPath={$page.url.pathname}
  {sections}
  brand="Kitu Admin"
  subtitle="Logic Processor"
  basePath={base}
  showRuntimeStatus={!isAdminHrefActive(
    $page.url.pathname,
    "/arena-inspector",
    base,
  )}
>
  {@render children?.()}
</AdminShell>
