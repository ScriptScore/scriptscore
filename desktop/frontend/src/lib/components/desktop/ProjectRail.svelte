<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import {
    AccountSetting01Icon,
    BookEditIcon,
    BookUserIcon,
    ChartBarLineIcon,
    CheckListIcon,
    DoorOpenIcon,
    FolderOpenIcon,
  } from '@hugeicons/core-free-icons';

  import type { TemplateSetupSubstep, WorkflowStep } from '$lib/stores/workspaceView';
  import { IconButton, SidebarRow, Surface } from './ui';

  export let activeWorkflowStep: WorkflowStep = 'templateSetup';
  /** When true for a step, show a subtle “needs attention” affordance on that rail icon. */
  export let attentionByStep: Partial<Record<WorkflowStep, boolean>> = {};
  export let hasDesktopHost = false;
  export let busy = false;
  export let onOpenProject: (() => void | Promise<void>) | null = null;
  export let onCloseProject: (() => void | Promise<void>) | null = null;
  export let onSelectWorkflowStep: ((step: WorkflowStep) => void) | null = null;
  export let onSelectTemplateSetupSubstep:
    | ((step: TemplateSetupSubstep) => void | Promise<void>)
    | null = null;

  let templateSetupMenuOpen = false;
  let templateSetupMenuCloseTimer: ReturnType<typeof setTimeout> | null = null;

  const workflowItems = [
    { id: 'templateSetup' as const, label: 'Template', icon: BookEditIcon, disabled: false },
    { id: 'students' as const, label: 'Students', icon: BookUserIcon, disabled: false },
    { id: 'moderation' as const, label: 'Moderation', icon: CheckListIcon, disabled: false },
    { id: 'exportResults' as const, label: 'Results', icon: ChartBarLineIcon, disabled: false }
  ];

  function clearTemplateSetupMenuCloseTimer() {
    if (templateSetupMenuCloseTimer !== null) {
      clearTimeout(templateSetupMenuCloseTimer);
      templateSetupMenuCloseTimer = null;
    }
  }

  function openTemplateSetupMenu() {
    clearTemplateSetupMenuCloseTimer();
    templateSetupMenuOpen = true;
  }

  function closeTemplateSetupMenu() {
    clearTemplateSetupMenuCloseTimer();
    templateSetupMenuOpen = false;
  }

  function scheduleTemplateSetupMenuClose() {
    clearTemplateSetupMenuCloseTimer();
    templateSetupMenuCloseTimer = setTimeout(() => {
      templateSetupMenuOpen = false;
      templateSetupMenuCloseTimer = null;
    }, 120);
  }

  function handleTemplateSetupFocusOut(event: FocusEvent) {
    const nextTarget = event.relatedTarget;
    const currentTarget = event.currentTarget;
    if (!(currentTarget instanceof HTMLElement)) {
      closeTemplateSetupMenu();
      return;
    }
    if (nextTarget instanceof Node && currentTarget.contains(nextTarget)) {
      return;
    }
    closeTemplateSetupMenu();
  }

  function handleTemplateSetupKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeTemplateSetupMenu();
    }
  }
</script>

<Surface
  as="aside"
  variant="rail"
  class="relative flex h-full flex-col items-center justify-between py-4 after:absolute after:bottom-0 after:right-0 after:top-14 after:w-px after:bg-border-subtle after:content-['']"
>
  <div class="flex w-full flex-col items-center gap-3">
    {#each workflowItems as item (item.id)}
      {#if item.id === 'templateSetup'}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="relative flex items-center"
          role="group"
          aria-label="Template navigation"
          onmouseenter={openTemplateSetupMenu}
          onmouseleave={scheduleTemplateSetupMenuClose}
          onfocusin={openTemplateSetupMenu}
          onfocusout={handleTemplateSetupFocusOut}
          onkeydown={handleTemplateSetupKeydown}
        >
          <IconButton
            variant="rail"
            size="rail"
            selected={activeWorkflowStep === item.id && !templateSetupMenuOpen}
            disabled={item.disabled}
            attention={attentionByStep[item.id] ?? false}
            type="button"
            title={item.label}
            ariaLabel={item.label}
            aria-haspopup="true"
            aria-expanded={templateSetupMenuOpen}
            onfocus={openTemplateSetupMenu}
            onclick={() => {
              if (!item.disabled) {
                onSelectWorkflowStep?.(item.id);
              }
            }}
          >
            <HugeiconsIcon icon={item.icon} size={22} strokeWidth={1.7} />
          </IconButton>

          {#if templateSetupMenuOpen}
            <div
              class="absolute left-full top-0 z-20 flex min-w-[11rem] flex-col gap-2 rounded-r-2xl border-y border-r border-border-subtle bg-surface-rail px-4 py-5"
              aria-label="Template steps"
              role="menu"
              aria-orientation="vertical"
              tabindex="-1"
              onmouseenter={openTemplateSetupMenu}
              onmouseleave={scheduleTemplateSetupMenuClose}
            >
              <div class="flex flex-col gap-2 px-1">
                <p class="text-xs font-semibold uppercase tracking-[0.24em] text-workspace-sidebar-foreground">
                  Template
                </p>
                <div class="h-px w-full bg-border-subtle"></div>
              </div>
              <SidebarRow
                class="min-h-10 px-3"
                role="menuitem"
                aria-label="Setup"
                onclick={() => {
                  void onSelectTemplateSetupSubstep?.('setup');
                }}
              >
                Setup
              </SidebarRow>
              <SidebarRow
                class="min-h-10 px-3"
                role="menuitem"
                aria-label="Review"
                onclick={() => {
                  void onSelectTemplateSetupSubstep?.('review');
                }}
              >
                Review
              </SidebarRow>
            </div>
          {/if}
        </div>
      {:else}
        <IconButton
          variant="rail"
          size="rail"
          selected={activeWorkflowStep === item.id}
          disabled={item.disabled}
          attention={attentionByStep[item.id] ?? false}
          type="button"
          title={item.label}
          ariaLabel={item.label}
          onclick={() => {
            if (!item.disabled) {
              onSelectWorkflowStep?.(item.id);
            }
          }}
        >
          <HugeiconsIcon icon={item.icon} size={22} strokeWidth={1.7} />
        </IconButton>
      {/if}
    {/each}
  </div>

  <div class="flex w-full flex-col items-center gap-3">
    <div class="h-px w-8 bg-border-subtle"></div>
    <IconButton
      variant="rail"
      size="rail"
      type="button"
      disabled={busy || !hasDesktopHost}
      title="Open Project"
      ariaLabel="Open Project"
      onclick={() => void onOpenProject?.()}
    >
      <HugeiconsIcon icon={FolderOpenIcon} size={22} strokeWidth={1.7} />
    </IconButton>
    <IconButton
      variant="rail"
      size="rail"
      type="button"
      disabled={busy || !hasDesktopHost}
      title="Close Project"
      ariaLabel="Close Project"
      onclick={() => void onCloseProject?.()}
    >
      <HugeiconsIcon icon={DoorOpenIcon} size={22} strokeWidth={1.7} />
    </IconButton>
    <IconButton
      variant="rail"
      size="rail"
      selected={activeWorkflowStep === 'settings'}
      type="button"
      disabled={busy || !hasDesktopHost}
      title="Settings"
      ariaLabel="Settings"
      onclick={() => {
        onSelectWorkflowStep?.('settings');
      }}
    >
      <HugeiconsIcon icon={AccountSetting01Icon} size={22} strokeWidth={1.7} />
    </IconButton>
  </div>
</Surface>
