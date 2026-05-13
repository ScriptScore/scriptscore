// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

const guardedFiles = [
  'src/routes/+page.svelte',
  'src/lib/stores/notifications.ts',
  'src/lib/RedactionCanvas.svelte',
  'src/lib/components/desktop/ProjectRail.svelte',
  'src/lib/components/desktop/ProjectTopBar.svelte',
  'src/lib/components/desktop/WorkspaceMessages.svelte',
  'src/lib/components/desktop/BlockedWorkspace.svelte',
  'src/lib/components/desktop/NoProjectScreen.svelte',
  'src/lib/components/desktop/FirstRunSetupWizard.svelte',
  'src/lib/components/desktop/ProjectModeStep.svelte',
  'src/lib/components/desktop/AiAssistanceStep.svelte',
  'src/lib/components/desktop/GradingProfileStep.svelte',
  'src/lib/components/desktop/CreateProjectPanel.svelte',
  'src/lib/components/desktop/RecentProjectsList.svelte',
  'src/lib/components/desktop/SettingsWorkspace.svelte',
  'src/lib/components/desktop/SettingsAutomationOptions.svelte',
  'src/lib/components/desktop/SettingsInstructorProfile.svelte',
  'src/lib/components/desktop/SettingsLlmProvider.svelte',
  'src/lib/components/desktop/SettingsLmsConfiguration.svelte',
  'src/lib/components/desktop/SettingsProjectStorage.svelte',
  'src/lib/components/desktop/SettingsRuntimeSmokeTest.svelte',
  'src/lib/components/desktop/SettingsThemeToggle.svelte',
  'src/lib/components/desktop/TraceHistoryDialog.svelte',
  'src/lib/components/desktop/ResultsWorkspace.svelte',
  'src/lib/components/desktop/ResultsReportPreview.svelte',
  'src/lib/components/desktop/ResultsMetricsSidebar.svelte',
  'src/lib/components/desktop/ResultsStudentsSidebar.svelte',
  'src/lib/components/desktop/NotificationToasts.svelte',
  'src/lib/components/desktop/TokenGuidePopover.svelte',
  'src/lib/components/desktop/ConnectionTicker.svelte',
  'src/lib/components/desktop/HorizontalProgressBar.svelte',
  'src/lib/components/desktop/QuestionDetailView.svelte',
  'src/lib/components/desktop/StudentWorkflowBoard.svelte',
  'src/lib/components/desktop/student-workflow-helpers.ts',
  'src/lib/components/desktop/ui/ConfirmDialog.svelte',
  'src/lib/components/desktop/ui/DesktopButton.svelte',
  'src/lib/components/desktop/ui/DesktopPopover.svelte',
  'src/lib/components/desktop/ui/ExportFormatDialog.svelte',
  'src/lib/components/desktop/ui/FieldShell.svelte',
  'src/lib/components/desktop/ui/IconButton.svelte',
  'src/lib/components/desktop/ui/IconSelectField.svelte',
  'src/lib/components/desktop/ui/ImageRegionEditor.svelte',
  'src/lib/components/desktop/ui/InlineMessage.svelte',
  'src/lib/components/desktop/ui/InteractiveCanvasSurface.svelte',
  'src/lib/components/desktop/ui/ListboxPopover.svelte',
  'src/lib/components/desktop/ui/PagePreviewFrame.svelte',
  'src/lib/components/desktop/ui/RadioCardGroup.svelte',
  'src/lib/components/desktop/ui/SegmentedControl.svelte',
  'src/lib/components/desktop/ui/SelectField.svelte',
  'src/lib/components/desktop/ui/SelectionHandle.svelte',
  'src/lib/components/desktop/ui/SideDrawer.svelte',
  'src/lib/components/desktop/ui/SidebarRow.svelte',
  'src/lib/components/desktop/ui/StatusBadge.svelte',
  'src/lib/components/desktop/ui/Surface.svelte',
  'src/lib/components/desktop/ui/TabStrip.svelte',
  'src/lib/components/desktop/ui/TextField.svelte',
  'src/lib/components/desktop/ui/TextareaField.svelte',
  'src/lib/components/desktop/ui/ToggleRow.svelte',
  'src/lib/components/desktop/ui/ToneIcon.svelte',
  'src/lib/components/desktop/ui/feedback.ts'
] as const;

const tailwindPalette =
  '(?:zinc|slate|gray|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|white|black)';

const forbiddenRecipes: Array<{ label: string; pattern: RegExp }> = [
  { label: 'legacy card background', pattern: /\bbg-card(?:\/|\b)/ },
  {
    label: 'hardcoded Tailwind palette color',
    pattern: new RegExp(
      `\\b(?:bg|text|border|ring|from|via|to|accent|decoration|outline|placeholder|caret|fill|stroke)-${tailwindPalette}(?:-|/|\\b)`
    )
  },
  { label: 'arbitrary literal color class', pattern: /\b[a-z]+-\[#/ },
  { label: 'arbitrary radius class', pattern: /\brounded-\[/ },
  { label: 'raw message variable class', pattern: /\b(?:bg|text|border)-\[var\(--message-/ }
];

const desktopSourceFiles = import.meta.glob('/src/**/*.{svelte,ts}', {
  query: '?raw',
  import: 'default',
  eager: true
}) as Record<string, string>;

const nativeConfirmationRecipes: Array<{ label: string; pattern: RegExp }> = [
  {
    label: 'Tauri confirmation dialog import',
    pattern:
      /import\s*\{[^}]*\bconfirm\b[^}]*\}\s*from\s*['"]@tauri-apps\/plugin-dialog['"]/
  },
  { label: 'browser confirmation dialog', pattern: /window\.confirm/ },
  { label: 'property confirmation dialog', pattern: /\b(?!window\.)[A-Za-z_$][\w$]*\.confirm\s*\(/ },
  { label: 'bare confirmation dialog', pattern: /(?<!\.)\bconfirm\s*\(/ }
];

function nativeConfirmationViolationsForSource(path: string, content: string): string[] {
  return content.split('\n').flatMap((line: string, index: number) =>
    nativeConfirmationRecipes.flatMap((recipe) => {
      const match = recipe.pattern.exec(line);
      if (!match) {
        return [];
      }

      return `${path.replace(/^\/src\//, 'src/')}:${index + 1} ${recipe.label}: ${match[0]}`;
    })
  );
}

function findUniformityViolations(): string[] {
  return guardedFiles.flatMap((file) => {
    const content = desktopSourceFiles[`/${file}`];
    if (typeof content !== 'string') {
      return [`${file}:0 guard setup: file is missing from raw source imports`];
    }

    return content.split('\n').flatMap((line: string, index: number) =>
      forbiddenRecipes.flatMap((recipe) => {
        const match = recipe.pattern.exec(line);
        if (!match) {
          return [];
        }

        return `${file}:${index + 1} ${recipe.label}: ${match[0]}`;
      })
    );
  });
}

function isProductionSourceFile(path: string): boolean {
  return (
    !path.includes('/src/test/') &&
    !path.endsWith('.test.ts') &&
    !path.endsWith('.d.ts')
  );
}

function findNativeConfirmationViolations(): string[] {
  return Object.entries(desktopSourceFiles)
    .filter(([path]) => isProductionSourceFile(path))
    .flatMap(([path, content]) => nativeConfirmationViolationsForSource(path, content));
}

describe('desktop UI uniformity guard', () => {
  it('keeps cleaned desktop surfaces on semantic token recipes', () => {
    expect(findUniformityViolations()).toEqual([]);
  });

  it('keeps production desktop source off native confirmation dialogs', () => {
    expect(findNativeConfirmationViolations()).toEqual([]);
  });

  it('catches Tauri confirmation imports and aliases', () => {
    expect(
      nativeConfirmationViolationsForSource(
        '/src/lib/example.svelte',
        "import { confirm as confirmDialog, open } from '@tauri-apps/plugin-dialog';"
      )
    ).toEqual([
      "src/lib/example.svelte:1 Tauri confirmation dialog import: import { confirm as confirmDialog, open } from '@tauri-apps/plugin-dialog'"
    ]);
  });

  it('catches browser and property confirmation calls', () => {
    expect(
      nativeConfirmationViolationsForSource(
        '/src/lib/example.ts',
        'window.confirm(message);\ndialog.confirm(message);\nconfirm(message);'
      )
    ).toEqual([
      'src/lib/example.ts:1 browser confirmation dialog: window.confirm',
      'src/lib/example.ts:2 property confirmation dialog: dialog.confirm(',
      'src/lib/example.ts:3 bare confirmation dialog: confirm('
    ]);
  });
});
