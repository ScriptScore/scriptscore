// SPDX-License-Identifier: AGPL-3.0-only
import { writable } from 'svelte/store';

import type { AppSettings } from '$lib/types';
import { theme } from './theme';

const STORAGE_KEY = 'scriptscore-app-settings';

export const defaultAppSettings: AppSettings = {
  llmProvider: 'ollama_native',
  llmBaseUrl: 'http://127.0.0.1:11434',
  llmModel: 'qwen3.5:9b',
  llmApiKey: null,
  lmsProvider: 'none',
  lmsCanvasBaseUrl: 'https://canvas.instructure.com',
  lmsCanvasApiKey: null,
  lmsBindingSecretPlaintextFallback: false,
  piiPaddleModelDir: null,
  preliminaryGradingMaxWorkers: 1,
  projectsDirectory: null,
  instructorProfile: {
    gradingStrictness: 'balanced',
    syntaxLeniency: 'medium',
    ocrTolerance: 'medium',
    partialCreditStyle: 'balanced',
    feedbackStyle: 'brief',
    additionalGuidance: '',
    includeMinimumCreditCriterion: false,
    minimumCreditPercent: 10
  },
  aiAssistEnabled: false,
  onboardingCompleted: false,
  aiAssistCategories: {
    rubrics: false,
    questionAnalysis: false,
    gradingFeedback: false,
    parsingReview: false
  },
  theme: 'dark'
};

/** Canvas provider selected and both base URL and API token are non-empty (trimmed). */
export function isCanvasLmsReady(settings: AppSettings): boolean {
  return (
    settings.lmsProvider === 'canvas' &&
    settings.lmsCanvasBaseUrl.trim().length > 0 &&
    (settings.lmsCanvasApiKey ?? '').trim().length > 0
  );
}

function createAppSettingsStore() {
  const { subscribe, set, update } = writable<AppSettings>(defaultAppSettings);

  function normalizePreliminaryGradingMaxWorkers(value: unknown): number {
    return typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= 4
      ? value
      : defaultAppSettings.preliminaryGradingMaxWorkers;
  }

  function init() {
    if (typeof globalThis.window === 'undefined') return;
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      set(defaultAppSettings);
      return;
    }
    try {
      const parsed = JSON.parse(raw) as Partial<AppSettings> & {
        instructorProfile?: Partial<AppSettings['instructorProfile']> & { minimumCreditPoints?: number };
        aiAssistCategories?: Partial<AppSettings['aiAssistCategories']>;
      };
      const ip = parsed.instructorProfile;
      const instructorProfile = (() => {
        if (!ip) return defaultAppSettings.instructorProfile;
        // Drop legacy minimumCreditPoints if present (migrated to minimumCreditPercent).
        const rest = { ...ip };
        delete rest.minimumCreditPoints;
        return {
          ...defaultAppSettings.instructorProfile,
          ...rest,
          minimumCreditPercent:
            typeof rest.minimumCreditPercent === 'number'
              ? rest.minimumCreditPercent
              : defaultAppSettings.instructorProfile.minimumCreditPercent
        };
      })();
      set({
        ...defaultAppSettings,
        ...parsed,
        preliminaryGradingMaxWorkers: normalizePreliminaryGradingMaxWorkers(
          parsed.preliminaryGradingMaxWorkers
        ),
        instructorProfile,
        aiAssistCategories: {
          ...defaultAppSettings.aiAssistCategories,
          rubrics:
            typeof parsed.aiAssistCategories?.rubrics === 'boolean'
              ? parsed.aiAssistCategories.rubrics
              : Boolean(parsed.aiAssistEnabled),
          questionAnalysis: parsed.aiAssistCategories?.questionAnalysis ?? Boolean(parsed.aiAssistEnabled),
          gradingFeedback: parsed.aiAssistCategories?.gradingFeedback ?? Boolean(parsed.aiAssistEnabled),
          parsingReview: parsed.aiAssistCategories?.parsingReview ?? Boolean(parsed.aiAssistEnabled)
        }
      });
    } catch {
      set(defaultAppSettings);
    }
  }

  function save(next: AppSettings) {
    if (typeof globalThis.window !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    }
    set(next);
    if (next.theme === 'dark' || next.theme === 'light') {
      theme.setExplicit(next.theme);
    }
  }

  return {
    subscribe,
    init,
    save,
    updateField<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
      update((current) => {
        const next = { ...current, [key]: value };
        if (typeof globalThis.window !== 'undefined') {
          localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
        }
        if (key === 'theme' && (value === 'dark' || value === 'light')) {
          theme.setExplicit(value);
        }
        return next;
      });
    },
    updateInstructorField<K extends keyof AppSettings['instructorProfile']>(
      key: K,
      value: AppSettings['instructorProfile'][K]
    ) {
      update((current) => {
        const next = {
          ...current,
          instructorProfile: {
            ...current.instructorProfile,
            [key]: value
          }
        };
        if (typeof globalThis.window !== 'undefined') {
          localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
        }
        return next;
      });
    }
  };
}

export const appSettings = createAppSettingsStore();
