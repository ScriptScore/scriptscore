<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { RecentProject } from './NoProjectScreen.types';

  export let recentProjects: RecentProject[] = [];
  export let onOpenRecentProject: ((projectPath: string) => void | Promise<void>) | null = null;
</script>

{#if recentProjects.length > 0}
  <div class="mx-auto mt-12 h-px max-w-5xl bg-border"></div>
  <section class="mx-auto mt-10 max-w-5xl">
    <div class="mb-4">
      <h2 class="shell-title-lg text-foreground">Recent Projects</h2>
    </div>
    <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {#each recentProjects as project (project.projectPath)}
        <button
          class="flex flex-col rounded-xl border border-border-default bg-surface-card-subtle px-4 py-3 text-left transition-colors hover:bg-surface-card-hover"
          type="button"
          onclick={() => void onOpenRecentProject?.(project.projectPath)}
        >
          <span class="truncate text-sm font-medium text-foreground">{project.displayName}</span>
          {#if project.courseCode}
            <span class="mt-0.5 text-xs text-muted-foreground">{project.courseCode}</span>
          {/if}
          <span class="mt-1 truncate text-xs text-muted-foreground">{project.projectPath}</span>
        </button>
      {/each}
    </div>
  </section>
{/if}
