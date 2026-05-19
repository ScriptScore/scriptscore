<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { HugeiconsIcon } from '@hugeicons/svelte';
  import {
    AccountSetting01Icon,
    Activity01Icon,
    AiMagicIcon,
    CopyrightIcon,
    ConnectIcon,
    FileSlidersIcon,
    FolderFileStorageIcon,
    InformationCircleIcon
  } from '@hugeicons/core-free-icons';

  import type {
    AppSettings,
    AppUpdateCheck,
    SmokePingResult,
    VisionCapableModel
  } from '$lib/types';
  import SettingsAutomationOptions from './SettingsAutomationOptions.svelte';
  import SettingsAbout from './SettingsAbout.svelte';
  import SettingsInstructorProfile from './SettingsInstructorProfile.svelte';
  import SettingsLegalDisclosure from './SettingsLegalDisclosure.svelte';
  import SettingsLlmProvider from './SettingsLlmProvider.svelte';
  import SettingsLmsConfiguration from './SettingsLmsConfiguration.svelte';
  import SettingsPreliminaryGradingWorkers from './SettingsPreliminaryGradingWorkers.svelte';
  import SettingsProjectStorage from './SettingsProjectStorage.svelte';
  import SettingsRuntimeSmokeTest from './SettingsRuntimeSmokeTest.svelte';
  import SettingsThemeToggle from './SettingsThemeToggle.svelte';
  import { SidebarRow, Surface } from './ui';

  export let settings: AppSettings;
  export let busy = false;
  export let smokeResult: SmokePingResult | null = null;
  export let appVersion: string | null = null;
  export let updateCheck: AppUpdateCheck | null = null;
  export let updateBusy = false;
  export let updateAvailable = false;
  export let visionModels: VisionCapableModel[] = [];
  export let visionModelsBusy = false;
  export let visionModelsError: string | null = null;
  export let hasDesktopHost = true;
  export let resolvedProjectsDirectory: string | null = null;
  export let hasCurrentProject = false;
  export let onSettingsChange: ((settings: AppSettings) => void | Promise<void>) | null = null;
  export let onRuntimeCheck: (() => void | Promise<void>) | null = null;
  export let onOpenTraceHistory: (() => void) | null = null;
  export let onChooseProjectsDirectory: (() => void | Promise<void>) | null = null;
  export let onClearProjectsDirectory: (() => void) | null = null;
  export let onChoosePiiPaddleModelDirectory: (() => void | Promise<void>) | null = null;
  export let onClearPiiPaddleModelDirectory: (() => void) | null = null;
  export let onRunSetupWizard: (() => void | Promise<void>) | null = null;
  export let onCheckForUpdates: (() => void | Promise<void>) | null = null;
  export let onDownloadUpdate: ((url: string) => void | Promise<void>) | null = null;

  type SettingsSection =
    | 'connections'
    | 'grading'
    | 'storage'
    | 'aiAssistance'
    | 'preferences'
    | 'diagnostics'
    | 'legal'
    | 'about';

  const settingsSections = [
    {
      id: 'connections',
      label: 'Connections',
      title: 'Connections',
      description: 'LLM provider and LMS integration settings.',
      icon: ConnectIcon
    },
    {
      id: 'aiAssistance',
      label: 'AI Assistance',
      title: 'AI Assistance',
      description: 'Enable AI-powered workflow steps by category.',
      icon: AiMagicIcon
    },
    {
      id: 'grading',
      label: 'Grading',
      title: 'Grading',
      description: 'Instructor defaults that shape AI-assisted grading.',
      icon: FileSlidersIcon
    },
    {
      id: 'storage',
      label: 'Storage',
      title: 'Storage',
      description: 'Local folders and model paths used by this desktop.',
      icon: FolderFileStorageIcon
    },
    {
      id: 'preferences',
      label: 'Preferences',
      title: 'Preferences',
      description: 'Personal workspace defaults.',
      icon: AccountSetting01Icon
    },
    {
      id: 'diagnostics',
      label: 'Diagnostics',
      title: 'Diagnostics',
      description: 'Runtime checks and worker traces.',
      icon: Activity01Icon
    },
    {
      id: 'legal',
      label: 'Legal',
      title: 'Legal',
      description: 'Source code offer and open source license notices.',
      icon: CopyrightIcon
    },
    {
      id: 'about',
      label: 'About',
      title: 'About',
      description: 'Version and updates.',
      icon: InformationCircleIcon
    }
  ] satisfies Array<{
    id: SettingsSection;
    label: string;
    title: string;
    description: string;
    icon: unknown;
  }>;

  let activeSection: SettingsSection = 'connections';

  $: activeSectionMeta =
    settingsSections.find((section) => section.id === activeSection) ?? settingsSections[0];

  function commitSettings() {
    void onSettingsChange?.(structuredClone(settings));
  }
</script>

<Surface as="section" variant="shell" class="flex h-full min-h-full overflow-hidden text-text-primary">
  <Surface
    as="aside"
    variant="sidebar"
    class="flex h-full min-h-full w-[16rem] shrink-0 flex-col border-r border-border-default px-3 py-5"
    aria-label="Settings sections"
  >
    <div class="px-2 pb-4 shell-eyebrow text-workspace-text-muted">Settings</div>
    <nav class="flex flex-col gap-2" aria-label="Settings categories">
      {#each settingsSections as section (section.id)}
        <SidebarRow
          active={activeSection === section.id}
          class={section.id === 'about' && updateAvailable ? '!border-message-info-border shadow-[0_0_0_1px_var(--message-info-border)] text-workspace-sidebar-foreground' : ''}
          aria-current={activeSection === section.id ? 'page' : undefined}
          onclick={() => {
            activeSection = section.id;
          }}
        >
          <HugeiconsIcon icon={section.icon} size={20} strokeWidth={1.8} aria-hidden="true" />
          <span>{section.id === 'about' && updateAvailable ? 'Update Available!' : section.label}</span>
        </SidebarRow>
      {/each}
    </nav>
  </Surface>

  <main class="min-w-0 flex-1 overflow-y-auto bg-surface-panel px-8 py-8">
    <div class="mx-auto flex max-w-4xl flex-col gap-8">
      {#if activeSection !== 'about'}
        <header>
          <div class="min-w-0">
            <h1 class="shell-title-lg font-medium tracking-tight text-workspace-text-primary">
              {activeSectionMeta.title}
            </h1>
            <p class="mt-1 max-w-3xl shell-body leading-6 text-workspace-text-secondary">
              {activeSectionMeta.description}
            </p>
          </div>
        </header>
      {/if}

      {#if activeSection === 'connections'}
        <div class="grid gap-7">
          <SettingsLlmProvider
            {settings}
            {visionModels}
            {visionModelsBusy}
            {visionModelsError}
            {hasDesktopHost}
            onChange={commitSettings}
          />
          <SettingsLmsConfiguration {settings} {hasDesktopHost} onChange={commitSettings} />
        </div>
      {:else if activeSection === 'grading'}
        <SettingsInstructorProfile {settings} onChange={commitSettings} />
      {:else if activeSection === 'storage'}
        <SettingsProjectStorage
          {settings}
          {busy}
          {resolvedProjectsDirectory}
          {onChooseProjectsDirectory}
          {onClearProjectsDirectory}
          {onChoosePiiPaddleModelDirectory}
          {onClearPiiPaddleModelDirectory}
          onChange={commitSettings}
        />
      {:else if activeSection === 'aiAssistance'}
        <SettingsAutomationOptions {settings} onChange={commitSettings} />
      {:else if activeSection === 'preferences'}
        <div class="grid gap-6">
          <SettingsThemeToggle {settings} onChange={commitSettings} />
          <SettingsPreliminaryGradingWorkers {settings} onChange={commitSettings} />
        </div>
      {:else if activeSection === 'diagnostics'}
        <SettingsRuntimeSmokeTest
          {busy}
          {smokeResult}
          {onRuntimeCheck}
          {onRunSetupWizard}
          traceHistoryDisabled={!hasCurrentProject}
          onOpenTraceHistory={() => {
            onOpenTraceHistory?.();
          }}
        />
      {:else if activeSection === 'legal'}
        <SettingsLegalDisclosure />
      {:else if activeSection === 'about'}
        <SettingsAbout
          {appVersion}
          {updateCheck}
          {updateBusy}
          {hasDesktopHost}
          {onCheckForUpdates}
          {onDownloadUpdate}
        />
      {/if}
    </div>
  </main>
</Surface>
