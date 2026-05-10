<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onDestroy } from 'svelte';

  import type { VisionCapableModel } from '$lib/types';
  import { appSettings } from '$lib/stores/appSettings';
  import AiAssistanceStep from './AiAssistanceStep.svelte';
  import {
    createCanvasOnboardingValidator,
    createOllamaOnboardingValidator
  } from './onboardingValidation';
  import ProjectModeStep from './ProjectModeStep.svelte';
  import { DesktopButton, Surface, feedbackToneClass } from './ui';

  export let hasDesktopHost = false;
  export let visionModels: VisionCapableModel[] = [];
  export let visionModelsBusy = false;
  export let onClose: (() => void) | null = null;
  export let onOpenSettings: (() => void) | null = null;

  const OLLAMA_LOCAL_BASE_URL = 'http://127.0.0.1:11434';
  const OLLAMA_LOCAL_DEFAULT_MODEL = 'qwen3.5:9b';
  const OLLAMA_CLOUD_BASE_URL = 'https://ollama.com/api';
  const OLLAMA_CLOUD_DEFAULT_MODEL = 'qwen3.5:cloud';

  const canvasValidation = createCanvasOnboardingValidator(() => hasDesktopHost);
  const ollamaValidation = createOllamaOnboardingValidator(() => hasDesktopHost);

  let activeOnboardingTask: 'project' | 'ai' | null = 'project';
  let projectTaskComplete = false;
  let aiTaskComplete = false;

  $: canvasBaseUrl = $appSettings.lmsCanvasBaseUrl.trim();
  $: canvasApiKey = ($appSettings.lmsCanvasApiKey ?? '').trim();
  $: canvasCredentialsKey = `${canvasBaseUrl}|${canvasApiKey}`;
  $: ollamaBaseUrl = $appSettings.llmBaseUrl.trim();
  $: ollamaApiKey = ($appSettings.llmApiKey ?? '').trim();
  $: ollamaCredentialsKey =
    $appSettings.llmProvider === 'ollama_cloud'
      ? `${$appSettings.llmProvider}|${ollamaBaseUrl}|${$appSettings.llmModel}|${ollamaApiKey}`
      : `${$appSettings.llmProvider}|${ollamaBaseUrl}|${$appSettings.llmModel}`;
  $: projectTaskReady =
    $appSettings.lmsProvider === 'none' ||
    ($appSettings.lmsProvider === 'canvas' &&
      $canvasValidation.connectionStatus === 'ok' &&
      $canvasValidation.validatedCredentialsKey === canvasCredentialsKey);
  $: aiTaskReady =
    !$appSettings.aiAssistEnabled ||
    (($appSettings.llmProvider === 'ollama_native' || $appSettings.llmProvider === 'ollama_cloud') &&
      $appSettings.llmModel.trim().length > 0 &&
      $ollamaValidation.connectionStatus === 'ok' &&
      $ollamaValidation.validatedCredentialsKey === ollamaCredentialsKey);
  $: ollamaValidationEnabled =
    $appSettings.aiAssistEnabled &&
    ($appSettings.llmProvider === 'ollama_cloud' ||
      ($appSettings.llmProvider === 'ollama_native' && !visionModelsBusy));
  $: finishSetupReady = projectTaskComplete && aiTaskComplete;
  $: finishSetupButtonClass = finishSetupReady
    ? '!border-message-success-border !bg-message-success-bg !text-message-success-text hover:!bg-message-success-bg hover:!text-message-success-text'
    : '';

  $: canvasValidation.handle($appSettings.lmsProvider, canvasBaseUrl, canvasApiKey);
  $: ollamaValidation.handle(
    ollamaValidationEnabled,
    $appSettings.llmProvider,
    ollamaBaseUrl,
    $appSettings.llmModel,
    ollamaApiKey
  );

  onDestroy(() => {
    canvasValidation.destroy();
    ollamaValidation.destroy();
  });

  function finishOnboarding() {
    appSettings.save({
      ...$appSettings,
      onboardingCompleted: true
    });
    onClose?.();
  }

  function updateSettings(patch: Partial<typeof $appSettings>) {
    appSettings.save({
      ...$appSettings,
      ...patch
    });
  }

  function chooseLocalProjectMode() {
    projectTaskComplete = false;
    updateSettings({
      lmsProvider: 'none',
      lmsCanvasApiKey: null
    });
  }

  function chooseCanvasProjectMode() {
    projectTaskComplete = false;
    updateSettings({
      lmsProvider: 'canvas'
    });
  }

  function disableAiAssist() {
    aiTaskComplete = false;
    updateSettings({
      aiAssistEnabled: false,
      aiAssistCategories: {
        rubrics: false,
        questionAnalysis: false,
        gradingFeedback: false,
        parsingReview: false
      }
    });
  }

  function enableAiAssist(provider: 'ollama_native' | 'ollama_cloud') {
    aiTaskComplete = false;
    const providerDefaults =
      provider === 'ollama_cloud'
        ? {
            llmBaseUrl: OLLAMA_CLOUD_BASE_URL,
            llmModel: OLLAMA_CLOUD_DEFAULT_MODEL
          }
        : {
            llmBaseUrl:
              $appSettings.llmBaseUrl.trim() === OLLAMA_CLOUD_BASE_URL
                ? OLLAMA_LOCAL_BASE_URL
                : $appSettings.llmBaseUrl,
            llmModel:
              $appSettings.llmModel.trim() === OLLAMA_CLOUD_DEFAULT_MODEL
                ? OLLAMA_LOCAL_DEFAULT_MODEL
                : $appSettings.llmModel,
            llmApiKey: null
          };
    updateSettings({
      aiAssistEnabled: true,
      llmProvider: provider,
      ...providerDefaults,
      aiAssistCategories: {
        rubrics: true,
        questionAnalysis: true,
        gradingFeedback: true,
        parsingReview: true
      }
    });
  }

  function completeProjectTask() {
    if (!projectTaskReady) {
      return;
    }
    projectTaskComplete = true;
    activeOnboardingTask = 'ai';
  }

  function completeAiTask() {
    if (!aiTaskReady) {
      return;
    }
    aiTaskComplete = true;
    activeOnboardingTask = null;
  }

  function onboardingTaskHeaderClass(active: boolean, complete: boolean) {
    return `flex w-full items-center justify-between gap-4 rounded-2xl px-4 py-3 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring ${
      complete
        ? 'bg-message-success-bg'
        : active
          ? 'bg-interaction-hover'
          : 'bg-surface-card-subtle hover:bg-surface-card-hover'
    }`;
  }

  function taskCheckClass(complete: boolean) {
    return `inline-flex size-7 shrink-0 items-center justify-center rounded-full border text-sm font-semibold ${
      complete
        ? feedbackToneClass('success')
        : 'border-border-default bg-surface-card-control text-text-secondary'
    }`;
  }

</script>

<Surface as="section" variant="cardRaised" bordered shadow="soft" radius="3xl" class="mx-auto mt-8 max-w-5xl">
  <div class="grid gap-7 px-7 py-6">
    <div>
      <div class="text-sm font-semibold uppercase tracking-[0.16em] text-muted-foreground">First-run setup</div>
      <h2 class="mt-2 shell-title-lg text-foreground">Configure ScriptScore</h2>
      <p class="mt-2 shell-body text-muted-foreground">
        Answer the core setup questions now. You can change LMS and AI choices later in Settings.
      </p>
    </div>

    <div class="grid gap-4">
      <section class="grid gap-2 border-t border-border/40 pt-4">
        <button
          class={onboardingTaskHeaderClass(activeOnboardingTask === 'project', projectTaskComplete)}
          type="button"
          onclick={() => {
            activeOnboardingTask = 'project';
          }}
        >
          <span class="flex min-w-0 items-center gap-3">
            <span class={taskCheckClass(projectTaskComplete)}>{projectTaskComplete ? '✓' : '1'}</span>
            <span class="min-w-0">
              <span class="block text-base font-semibold text-foreground">Project mode</span>
              <span class="block truncate text-sm text-muted-foreground">
                {$appSettings.lmsProvider === 'none' ? 'Local project setup' : 'LMS-linked setup'}
              </span>
            </span>
          </span>
          {#if projectTaskComplete}
            <span class="shrink-0 text-sm font-medium text-message-success-text">Complete</span>
          {/if}
        </button>

        {#if activeOnboardingTask === 'project'}
          <ProjectModeStep
            ready={projectTaskReady}
            message={$canvasValidation.displayedMessage}
            onChooseLocal={chooseLocalProjectMode}
            onChooseCanvas={chooseCanvasProjectMode}
            onCanvasBaseUrlInput={(value) => {
              projectTaskComplete = false;
              updateSettings({ lmsCanvasBaseUrl: value });
            }}
            onCanvasTokenInput={(value) => {
              projectTaskComplete = false;
              updateSettings({ lmsCanvasApiKey: value });
            }}
            onContinue={completeProjectTask}
          />
        {/if}
      </section>

      <section class="grid gap-2 border-t border-border/40 pt-4">
        <button
          class={onboardingTaskHeaderClass(activeOnboardingTask === 'ai', aiTaskComplete)}
          type="button"
          onclick={() => {
            if (projectTaskComplete) {
              activeOnboardingTask = 'ai';
            }
          }}
          disabled={!projectTaskComplete}
        >
          <span class="flex min-w-0 items-center gap-3">
            <span class={taskCheckClass(aiTaskComplete)}>{aiTaskComplete ? '✓' : '2'}</span>
            <span class="min-w-0">
              <span class="block text-base font-semibold text-foreground">AI assistance</span>
              <span class="block truncate text-sm text-muted-foreground">
                {$appSettings.aiAssistEnabled ? `${$appSettings.llmProvider === 'ollama_cloud' ? 'Ollama Cloud' : 'Local Ollama'} enabled` : 'No AI assistance'}
              </span>
            </span>
          </span>
          {#if aiTaskComplete}
            <span class="shrink-0 text-sm font-medium text-message-success-text">Complete</span>
          {:else if !projectTaskComplete}
            <span class="shrink-0 text-sm text-muted-foreground">Locked</span>
          {/if}
        </button>

        {#if activeOnboardingTask === 'ai' && projectTaskComplete}
          <AiAssistanceStep
            ready={aiTaskReady}
            message={$ollamaValidation.displayedMessage}
            {visionModels}
            visionModelsBusy={visionModelsBusy}
            onDisableAiAssist={disableAiAssist}
            onEnableAiAssist={enableAiAssist}
            onLlmBaseUrlInput={(value) => {
              aiTaskComplete = false;
              updateSettings({ llmBaseUrl: value });
            }}
            onLlmModelInput={(value) => {
              aiTaskComplete = false;
              updateSettings({ llmModel: value });
            }}
            onLlmApiKeyInput={(value) => {
              aiTaskComplete = false;
              updateSettings({ llmApiKey: value });
            }}
            onContinue={completeAiTask}
          />
        {/if}
      </section>
    </div>

    <div class="flex flex-wrap justify-end gap-3">
      <DesktopButton size="large" onclick={() => onOpenSettings?.()}>
        Open Settings
      </DesktopButton>
      <DesktopButton size="large" onclick={finishOnboarding}>
        Skip
      </DesktopButton>
      <DesktopButton
        class={finishSetupButtonClass}
        size="large"
        disabled={!finishSetupReady}
        onclick={finishOnboarding}
      >
        Finish Setup
      </DesktopButton>
    </div>
  </div>
</Surface>
