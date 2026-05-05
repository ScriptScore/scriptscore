// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it } from 'vitest';
import { get } from 'svelte/store';

import { appSettings, defaultAppSettings, isCanvasLmsReady } from './appSettings';
import type { AppSettings } from '$lib/types';

describe('isCanvasLmsReady', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove('light');
    appSettings.save(defaultAppSettings);
  });

  it('defaults the LMS binding secret plaintext fallback to off', () => {
    expect(defaultAppSettings.lmsBindingSecretPlaintextFallback).toBe(false);
  });

  it('defaults first-run setup and AI assistance categories conservatively', () => {
    expect(defaultAppSettings.projectsDirectory).toBeNull();
    expect(defaultAppSettings.onboardingCompleted).toBe(false);
    expect(defaultAppSettings.preliminaryGradingMaxWorkers).toBe(1);
    expect(defaultAppSettings.aiAssistCategories).toEqual({
      rubrics: false,
      questionAnalysis: false,
      gradingFeedback: false,
      parsingReview: false
    });
  });

  it('hydrates legacy AI Assist settings into all assistance categories', () => {
    localStorage.setItem(
      'scriptscore-app-settings',
      JSON.stringify({
        ...defaultAppSettings,
        aiAssistEnabled: true,
        aiAssistCategories: undefined
      })
    );

    appSettings.init();

    expect(get(appSettings).aiAssistCategories.rubrics).toBe(true);
    expect(get(appSettings).aiAssistCategories.questionAnalysis).toBe(true);
    expect(get(appSettings).aiAssistCategories.gradingFeedback).toBe(true);
    expect(get(appSettings).aiAssistCategories.parsingReview).toBe(true);
  });

  it('initializes defaults with empty or malformed local storage', () => {
    localStorage.clear();
    appSettings.updateField('llmModel', 'changed-model');
    localStorage.clear();

    appSettings.init();
    expect(get(appSettings)).toEqual(defaultAppSettings);

    appSettings.updateField('llmModel', 'changed-again');
    localStorage.setItem('scriptscore-app-settings', '{malformed json');

    appSettings.init();
    expect(get(appSettings)).toEqual(defaultAppSettings);
  });

  it('hydrates partial instructor profiles and drops legacy minimum credit points', () => {
    localStorage.setItem(
      'scriptscore-app-settings',
      JSON.stringify({
        ...defaultAppSettings,
        instructorProfile: {
          gradingStrictness: 'strict',
          additionalGuidance: 'Use concise comments.',
          minimumCreditPoints: 2
        }
      })
    );

    appSettings.init();

    expect(get(appSettings).instructorProfile).toEqual({
      ...defaultAppSettings.instructorProfile,
      gradingStrictness: 'strict',
      additionalGuidance: 'Use concise comments.'
    });
    expect('minimumCreditPoints' in get(appSettings).instructorProfile).toBe(false);
  });

  it('hydrates legacy settings with serial preliminary grading workers', () => {
    const legacySettings: Partial<AppSettings> = structuredClone(defaultAppSettings);
    delete legacySettings.preliminaryGradingMaxWorkers;
    localStorage.setItem('scriptscore-app-settings', JSON.stringify(legacySettings));

    appSettings.init();

    expect(get(appSettings).preliminaryGradingMaxWorkers).toBe(1);
  });

  it('falls back to serial preliminary grading workers for invalid saved values', () => {
    for (const preliminaryGradingMaxWorkers of [0, 5, 2.5, '4']) {
      localStorage.setItem(
        'scriptscore-app-settings',
        JSON.stringify({
          ...defaultAppSettings,
          preliminaryGradingMaxWorkers
        })
      );

      appSettings.init();
      expect(get(appSettings).preliminaryGradingMaxWorkers).toBe(1);
    }
  });

  it('persists save and field updates to local storage', () => {
    const saved = {
      ...defaultAppSettings,
      llmModel: 'qwen3:14b',
      onboardingCompleted: true
    };

    appSettings.save(saved);
    expect(JSON.parse(localStorage.getItem('scriptscore-app-settings') ?? '{}')).toMatchObject({
      llmModel: 'qwen3:14b',
      onboardingCompleted: true
    });

    appSettings.updateField('projectsDirectory', '/tmp/scriptscore-projects');
    expect(JSON.parse(localStorage.getItem('scriptscore-app-settings') ?? '{}')).toMatchObject({
      projectsDirectory: '/tmp/scriptscore-projects'
    });

    appSettings.updateInstructorField('feedbackStyle', 'detailed');
    expect(JSON.parse(localStorage.getItem('scriptscore-app-settings') ?? '{}')).toMatchObject({
      instructorProfile: expect.objectContaining({
        feedbackStyle: 'detailed'
      })
    });
  });

  it('applies explicit theme updates through local storage and the document class', () => {
    appSettings.save({
      ...defaultAppSettings,
      theme: 'light'
    });

    expect(localStorage.getItem('scriptscore-theme')).toBe('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);

    appSettings.updateField('theme', 'dark');

    expect(localStorage.getItem('scriptscore-theme')).toBe('dark');
    expect(document.documentElement.classList.contains('light')).toBe(false);
  });

  it('is false when provider is not canvas', () => {
    expect(isCanvasLmsReady({ ...defaultAppSettings, lmsProvider: 'none' })).toBe(false);
  });

  it('is false when canvas is selected but URL or token is empty', () => {
    expect(
      isCanvasLmsReady({
        ...defaultAppSettings,
        lmsProvider: 'canvas',
        lmsCanvasBaseUrl: '   ',
        lmsCanvasApiKey: ''
      })
    ).toBe(false);
  });

  it('is true when canvas URL and token are set', () => {
    expect(
      isCanvasLmsReady({
        ...defaultAppSettings,
        lmsProvider: 'canvas',
        lmsCanvasBaseUrl: 'https://school.instructure.com',
        lmsCanvasApiKey: 'token'
      })
    ).toBe(true);
  });
});
