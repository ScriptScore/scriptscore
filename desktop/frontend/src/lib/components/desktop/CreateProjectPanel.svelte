<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import { onMount, tick } from 'svelte';

  import { listCanvasCourses } from '$lib/desktop';
  import { appSettings, isCanvasLmsReady } from '$lib/stores/appSettings';
  import { jobProgress } from '$lib/stores/shell';
  import type { BusyAction } from '$lib/stores/workspaceView';
  import type { CreateProjectInput, LmsCourseSummary } from '$lib/types';
  import { DesktopButton, SelectField, TextField } from './ui';

  export let hasDesktopHost = false;
  export let busyAction: BusyAction = null;
  export let createInput: CreateProjectInput;
  export let scrollOnMount = false;
  export let onHideCreateForm: (() => void) | null = null;
  export let onChooseTemplatePdfForCreate: (() => void | Promise<void>) | null = null;
  export let onSubmitCreate: (() => void | Promise<void>) | null = null;

  let lmsCourses: LmsCourseSummary[] = [];
  let lmsCoursesError: string | null = null;
  let lmsCoursesLoading = false;
  let createFormSection: HTMLElement | null = null;

  $: canvasLmsReady = isCanvasLmsReady($appSettings);
  $: courseOptions = lmsCourses.map((course) => ({
    value: course.lmsCourseId,
    label: course.name
  }));
  $: if (hasDesktopHost && canvasLmsReady) {
    void refreshLmsCourses();
  }

  onMount(() => {
    if (scrollOnMount) {
      void scrollCreateFormIntoView();
    }
  });

  async function scrollCreateFormIntoView() {
    await tick();
    if (typeof createFormSection?.scrollIntoView === 'function') {
      createFormSection.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  }

  async function refreshLmsCourses() {
    const url = $appSettings.lmsCanvasBaseUrl.trim();
    const token = ($appSettings.lmsCanvasApiKey ?? '').trim();
    lmsCoursesLoading = true;
    lmsCoursesError = null;
    try {
      lmsCourses = await listCanvasCourses(url, token);
    } catch (error) {
      lmsCourses = [];
      lmsCoursesError = String(error);
    } finally {
      lmsCoursesLoading = false;
    }
  }

  function selectCreateLmsCourse(lmsCourseId: string) {
    const row = lmsCourses.find((course) => course.lmsCourseId === lmsCourseId);
    if (row) {
      createInput.lmsCourseId = row.lmsCourseId;
      createInput.courseCode = row.courseCode ?? '';
    } else {
      createInput.lmsCourseId = null;
      createInput.courseCode = '';
    }
  }
</script>

<section
  bind:this={createFormSection}
  class="mx-auto mt-12 max-w-3xl rounded-3xl border border-border bg-surface-card shadow-[var(--surface-shadow-soft)]"
>
  <div class="px-7 py-6">
    <h2 class="shell-title-lg text-foreground">Create a Project</h2>
    <p class="mt-1 shell-body text-muted-foreground">
      Select the exam name and template PDF. ScriptScore will create the project and immediately run template setup.
    </p>
  </div>
  <form
    class="grid gap-5 px-7 pb-6"
    onsubmit={(event) => {
      event.preventDefault();
      void onSubmitCreate?.();
    }}
  >
    <div class="grid gap-5 sm:grid-cols-3">
      <div class="grid gap-2.5">
        <TextField
          id="displayName"
          label="Exam Name"
          density="large"
          bind:value={createInput.displayName}
          placeholder="US History Exam"
          required
        />
      </div>
      <div class="grid gap-2.5">
        <TextField
          id="subject"
          label="Subject"
          density="large"
          value={createInput.subject ?? ''}
          placeholder="History"
          oninput={(event: Event) => {
            createInput.subject = (event.currentTarget as HTMLInputElement).value;
          }}
        />
      </div>
      <div class="grid gap-2.5">
        {#if canvasLmsReady}
          <div class="text-lg font-semibold text-foreground">Course</div>
          {#if lmsCoursesLoading}
            <div class="flex min-h-12 w-full rounded-xl border border-border-default bg-workspace-empty px-4 py-3 text-base text-text-muted shadow-[var(--surface-shadow-inset)]">Loading courses...</div>
          {:else if lmsCoursesError}
            <div class="rounded-xl border border-message-error-border bg-message-error-bg px-3 py-2 text-sm text-message-error-text">
              {lmsCoursesError}
            </div>
          {:else}
            <SelectField
              id="lmsCourse"
              density="large"
              value={createInput.lmsCourseId ?? ''}
              options={courseOptions}
              searchable
              searchPlaceholder="Search courses"
              allowEmpty
              emptyLabel="No LMS course"
              placeholder="No LMS course"
              noOptionsLabel="No matching courses."
              ariaLabel="Course"
              onChange={selectCreateLmsCourse}
            />
          {/if}
        {:else}
          <TextField
            id="courseCode"
            label="Course Code"
            density="large"
            value={createInput.courseCode ?? ''}
            placeholder="HIST 201"
            oninput={(event: Event) => {
              createInput.courseCode = (event.currentTarget as HTMLInputElement).value;
            }}
          />
          {#if $appSettings.lmsProvider === 'canvas' && !canvasLmsReady}
            <p class="text-sm text-muted-foreground">
              Add a Canvas base URL and API token in Settings to choose a course from Canvas.
            </p>
          {/if}
        {/if}
      </div>
    </div>
    <div class="grid gap-2.5">
      <div class="text-lg font-semibold text-foreground">Template PDF</div>
      <div class="flex flex-col gap-3 sm:flex-row sm:items-center">
        <DesktopButton
          class="min-w-[9.5rem]"
          size="large"
          disabled={busyAction === 'create'}
          onclick={() => void onChooseTemplatePdfForCreate?.()}
        >
          Choose PDF
        </DesktopButton>
        <div class="flex min-h-12 flex-1 items-center break-all rounded-xl border border-border-default bg-workspace-empty px-4 py-3 text-base text-text-primary shadow-[var(--surface-shadow-inset)]">
          {createInput.templatePdfPath || 'No template selected yet.'}
        </div>
      </div>
    </div>
    <div class="mt-1 h-px bg-border"></div>
    <div class="flex flex-wrap items-center justify-between gap-3 pt-1">
      <DesktopButton
        size="large"
        disabled={busyAction === 'create'}
        onclick={() => {
          onHideCreateForm?.();
        }}
      >
        Cancel
      </DesktopButton>
      <DesktopButton size="large" type="submit" disabled={busyAction === 'create' || !createInput.templatePdfPath}>
        {#if busyAction === 'create'}
          Creating Project... {$jobProgress ?? 0}%
        {:else}
          Create Project
        {/if}
      </DesktopButton>
    </div>
  </form>
</section>
