<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { RecentProject } from './NoProjectScreen.types';

  export let recentProjects: RecentProject[] = [];
  export let onOpenRecentProject: ((projectPath: string) => void | Promise<void>) | null = null;

  function trimTrailingSeparators(projectPath: string): string {
    let end = projectPath.length;
    while (end > 0 && (projectPath[end - 1] === '/' || projectPath[end - 1] === '\\')) {
      end -= 1;
    }
    return projectPath.slice(0, end);
  }

  function splitProjectPath(projectPath: string): { directory: string; folder: string } {
    const trimmedPath = trimTrailingSeparators(projectPath);
    const separatorIndex = Math.max(trimmedPath.lastIndexOf('/'), trimmedPath.lastIndexOf('\\'));
    if (separatorIndex < 0) {
      return { directory: '', folder: trimmedPath || projectPath };
    }
    return {
      directory: trimmedPath.slice(0, separatorIndex + 1),
      folder: trimmedPath.slice(separatorIndex + 1) || trimmedPath
    };
  }
</script>

{#if recentProjects.length > 0}
  <div class="mx-auto mt-12 h-px max-w-5xl bg-border"></div>
  <section class="mx-auto mt-10 max-w-5xl">
    <div class="mb-4">
      <h2 class="shell-title-lg text-foreground">Recent Projects</h2>
    </div>
    <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {#each recentProjects as project (project.projectPath)}
        {@const pathParts = splitProjectPath(project.projectPath)}
        <button
          class="flex min-w-0 flex-col rounded-xl border border-border-default bg-surface-card-subtle px-4 py-3 text-left transition-colors hover:bg-surface-card-hover"
          type="button"
          title={project.projectPath}
          onclick={() => void onOpenRecentProject?.(project.projectPath)}
        >
          <span class="truncate text-sm font-medium text-foreground">{project.displayName}</span>
          {#if project.courseCode}
            <span class="mt-0.5 text-xs text-muted-foreground">{project.courseCode}</span>
          {/if}
          <span
            class="mt-1 flex w-full min-w-0 justify-end text-xs text-muted-foreground"
            aria-label={project.projectPath}
          >
            {#if pathParts.directory}
              <span class="min-w-0 truncate text-left [direction:rtl] [unicode-bidi:plaintext]">
                {pathParts.directory}
              </span>
            {/if}
            <span class="shrink-0">{pathParts.folder}</span>
          </span>
        </button>
      {/each}
    </div>
  </section>
{/if}
