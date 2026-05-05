<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { CreateProjectInput, VisionCapableModel } from '$lib/types';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import { appSettings } from '$lib/stores/appSettings';
  import AppMark from './AppMark.svelte';
  import CreateProjectPanel from './CreateProjectPanel.svelte';
  import FirstRunSetupWizard from './FirstRunSetupWizard.svelte';
  import NotificationToasts from './NotificationToasts.svelte';
  import type { RecentProject } from './NoProjectScreen.types';
  import RecentProjectsList from './RecentProjectsList.svelte';
  import { DesktopButton, InlineMessage } from './ui';

  export let hasDesktopHost = false;
  export let showCreateForm = false;
  export let busyAction: BusyAction = null;
  export let createInput: CreateProjectInput;
  export let actionError: string | null = null;
  export let recentProjects: RecentProject[] = [];
  export let visionModels: VisionCapableModel[] = [];
  export let visionModelsBusy = false;
  export let forceOnboardingOpen = false;
  export let onShowCreateForm: (() => void) | null = null;
  export let onHideCreateForm: (() => void) | null = null;
  export let onOpenProject: (() => void | Promise<void>) | null = null;
  export let onOpenRecentProject: ((projectPath: string) => void | Promise<void>) | null = null;
  export let onChooseTemplatePdfForCreate: (() => void | Promise<void>) | null = null;
  export let onSubmitCreate: (() => void | Promise<void>) | null = null;
  export let onCloseOnboarding: (() => void) | null = null;
  export let onOpenSettings: (() => void) | null = null;

  let previousShowCreateForm = showCreateForm;
  let scrollCreateFormOnMount = false;

  $: showOnboarding = hasDesktopHost && (forceOnboardingOpen || !$appSettings.onboardingCompleted);
  $: handleCreateFormVisibility(showCreateForm);

  function handleCreateFormVisibility(nextShowCreateForm: boolean) {
    if (nextShowCreateForm === previousShowCreateForm) {
      scrollCreateFormOnMount = false;
      return;
    }
    scrollCreateFormOnMount = nextShowCreateForm && !previousShowCreateForm;
    previousShowCreateForm = nextShowCreateForm;
  }
</script>

<div class="no-project-screen min-h-screen">
  <header class="sticky top-0 z-10 border-b border-border/40 bg-surface-app/95 backdrop-blur">
    <div class="flex w-full items-center justify-between gap-4 px-4 py-4">
      <div class="flex min-w-0 items-center gap-3">
        <AppMark size={32} alt="" className="size-8 shrink-0" />
        <div class="min-w-0">
          <div class="text-base font-semibold tracking-tight">ScriptScore Desktop</div>
        </div>
      </div>
      <div class="flex min-w-0 items-center gap-3">
        <NotificationToasts placement="topbar" />
        <DesktopButton
          class="min-w-[5.5rem] rounded-full"
          size="compact"
          onclick={() => onOpenSettings?.()}
          title="App settings"
        >
          Settings
        </DesktopButton>
      </div>
    </div>
  </header>

  <main class="mx-auto flex w-full max-w-7xl flex-1 flex-col px-6 py-10">
    <section class="flex min-h-[calc(100vh-9rem)] items-center justify-center">
      <div class="w-full max-w-5xl">
        {#if !showOnboarding}
          <div class="mx-auto flex max-w-3xl flex-col items-center text-center">
            <AppMark size={112} alt="" className="mb-6 size-28" />
            <h1 class="shell-hero text-foreground">Let&apos;s Score an Exam!</h1>
            <p class="mt-5 max-w-2xl text-balance shell-body-lg text-muted-foreground">
              Create a new project in your default projects folder or open an existing project directory
              that already contains a <code class="rounded bg-surface-message px-1.5 py-0.5 text-sm font-mono">scriptscore.db</code>.
            </p>
            <div class="mt-10 flex flex-wrap items-center justify-center gap-4">
              <DesktopButton
                class="min-w-[11.5rem]"
                size="large"
                disabled={busyAction !== null || !hasDesktopHost}
                onclick={() => onShowCreateForm?.()}
              >
                Create Project
              </DesktopButton>
              <DesktopButton
                class="min-w-[11.5rem]"
                size="large"
                disabled={busyAction !== null || !hasDesktopHost}
                onclick={() => void onOpenProject?.()}
              >
                Open Project
              </DesktopButton>
            </div>
          </div>
        {/if}

        {#if !hasDesktopHost}
          <div class="mx-auto mt-8 max-w-3xl rounded-3xl border border-border bg-surface-card px-5 py-4 shadow-[var(--surface-shadow-soft)]">
            <div class="text-base font-semibold">Browser preview mode</div>
            <p class="mt-1 text-base text-muted-foreground">
              This window is only rendering the frontend. Create Project, Open Project, and Runtime Check require the Tauri desktop host.
            </p>
          </div>
        {/if}

        {#if showOnboarding}
          <FirstRunSetupWizard
            {hasDesktopHost}
            {visionModels}
            visionModelsBusy={visionModelsBusy}
            onClose={onCloseOnboarding}
            onOpenSettings={onOpenSettings}
          />
        {/if}

        {#if showCreateForm && !showOnboarding}
          <CreateProjectPanel
            {hasDesktopHost}
            {busyAction}
            {createInput}
            scrollOnMount={scrollCreateFormOnMount}
            {onHideCreateForm}
            {onChooseTemplatePdfForCreate}
            {onSubmitCreate}
          />
        {/if}

        {#if !showOnboarding}
          <RecentProjectsList {recentProjects} {onOpenRecentProject} />
        {/if}

        {#if actionError}
          <InlineMessage tone="error" class="mx-auto mt-6 max-w-5xl rounded-2xl px-5 py-4 text-base">
            <div class="text-base font-semibold">Action failed</div>
            <p class="mt-1 text-base">{actionError}</p>
          </InlineMessage>
        {/if}
      </div>
    </section>
  </main>
</div>
