<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<script lang="ts">
  import type { AppSettings } from '$lib/types';
  import { ToggleRow } from './ui';

  export let settings: AppSettings;
  export let onChange: (() => void | Promise<void>) | null = null;

  function toggleAiAssistCategory(key: keyof AppSettings['aiAssistCategories']) {
    settings.aiAssistCategories = {
      ...settings.aiAssistCategories,
      [key]: !settings.aiAssistCategories[key]
    };
    settings.aiAssistEnabled = Object.values(settings.aiAssistCategories).some(Boolean);
    void onChange?.();
  }
</script>

<section class="space-y-4">
  <div>
    <div class="shell-eyebrow text-workspace-text-muted">AI workflow steps</div>
    <div class="mt-1 text-sm leading-7 text-workspace-text-secondary">
      Enable automated workflow actions by category.
    </div>
  </div>
  <div class="divide-y divide-workspace-border border-y border-workspace-border">
    <ToggleRow
      class="min-h-16 py-3 hover:bg-surface-card-hover"
      title="Exam Analysis"
      description="Run exam.analyze after project setup to interpret question text and context."
      checked={settings.aiAssistCategories.questionAnalysis}
      onToggle={() => toggleAiAssistCategory('questionAnalysis')}
    />
    <ToggleRow
      class="min-h-16 py-3 hover:bg-surface-card-hover"
      title="Rubric Generation"
      description="Generate rubric drafts after exam analysis completes."
      checked={settings.aiAssistCategories.rubrics}
      onToggle={() => toggleAiAssistCategory('rubrics')}
    />
    <ToggleRow
      class="min-h-16 py-3 hover:bg-surface-card-hover"
      title="Grading"
      description="Use AI for preliminary scoring and markup."
      checked={settings.aiAssistCategories.parsingReview}
      onToggle={() => toggleAiAssistCategory('parsingReview')}
    />
    <ToggleRow
      class="min-h-16 py-3 hover:bg-surface-card-hover"
      title="Feedback"
      description="Allow assisted feedback text during grading review."
      checked={settings.aiAssistCategories.gradingFeedback}
      onToggle={() => toggleAiAssistCategory('gradingFeedback')}
    />
  </div>
</section>
