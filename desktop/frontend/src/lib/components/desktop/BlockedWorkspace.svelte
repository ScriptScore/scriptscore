<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { InlineMessage, Surface } from './ui';

  export let title = '';
  export let summary = '';
  export let prerequisites: readonly string[] = [];
  export let implementedIn = '';
</script>

<section class="flex h-full min-h-0 items-center justify-center bg-surface-panel px-8 py-8">
  <Surface class="w-full max-w-5xl p-8" variant="canvas" bordered radius="3xl" shadow="strong">
    <div class="shell-eyebrow text-muted-foreground">Workspace unavailable</div>
    <h1 class="mt-4 shell-hero text-foreground">{title}</h1>
    <p class="mt-4 max-w-3xl shell-body-lg text-muted-foreground">{summary}</p>

    <div class="mt-10 grid gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]">
      <InlineMessage class="rounded-2xl p-6" tone="warning">
        <div class="shell-section-title">Prerequisites</div>
        <div class="mt-4 space-y-3">
          {#each prerequisites as prerequisite (prerequisite)}
            <div class="rounded-2xl border border-message-warning-border bg-surface-message px-4 py-3 text-sm leading-6">
              {prerequisite}
            </div>
          {/each}
        </div>
      </InlineMessage>

      <Surface class="p-6" variant="cardSubtle" bordered radius="2xl">
        <div class="shell-section-title text-muted-foreground">Phase ownership</div>
        <div class="mt-4 shell-title text-foreground">{implementedIn}</div>
        <p class="mt-4 shell-body text-muted-foreground">
          This destination stays navigable now so operators can see blockers in context instead of
          encountering disabled navigation.
        </p>
      </Surface>
    </div>
  </Surface>
</section>
