// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from 'vitest';

import {
  commandWorkflowStage,
  commandProgressRange,
  interpolateProgress,
  isSubstageBoundaryProgressPayload,
  normalizedWorkflowCommandPercent,
  readCliProgressPercent,
  stageProgressTone,
  stageLabel,
  updateWorkflowCommandProgressState,
  workflowBarValueFromCommandProgress
} from './student-workflow-helpers';

describe('readCliProgressPercent', () => {
  it('returns numeric percent when present', () => {
    expect(readCliProgressPercent({ progress: { percent: 42 } })).toBe(42);
  });

  it('derives percent from completed and total like the CLI', () => {
    expect(readCliProgressPercent({ progress: { completed: 1, total: 4 } })).toBe(25);
    expect(readCliProgressPercent({ progress: { completed: 3, total: 10 } })).toBe(30);
    expect(readCliProgressPercent({ progress: { completed: 2, total: 3 } })).toBe(66);
  });

  it('prefers explicit percent over completed/total', () => {
    expect(
      readCliProgressPercent({ progress: { percent: 50, completed: 9, total: 10 } })
    ).toBe(50);
  });

  it('returns null when progress is missing or unusable', () => {
    expect(readCliProgressPercent({})).toBeNull();
    expect(readCliProgressPercent({ progress: null })).toBeNull();
    expect(readCliProgressPercent({ progress: 'x' as unknown as Record<string, unknown> })).toBeNull();
    expect(readCliProgressPercent({ progress: {} })).toBeNull();
    expect(readCliProgressPercent({ progress: { completed: 1, total: 0 } })).toBeNull();
  });
});

describe('isSubstageBoundaryProgressPayload', () => {
  it('is true for multi-stage stage_started without progress summary', () => {
    const state = { totalStages: 2, currentStage: 2 };
    expect(
      isSubstageBoundaryProgressPayload(
        { event: 'stage_started', data: { stage_number: 2 } },
        state
      )
    ).toBe(true);
  });

  it('is false when progress percent is present', () => {
    const state = { totalStages: 2, currentStage: 1 };
    expect(
      isSubstageBoundaryProgressPayload(
        { event: 'stage_started', progress: { percent: 0 }, data: { stage_number: 2 } },
        state
      )
    ).toBe(false);
  });

  it('is false for single-stage commands', () => {
    const state = { totalStages: 1, currentStage: 1 };
    expect(
      isSubstageBoundaryProgressPayload({ event: 'stage_started', data: { stage_number: 1 } }, state)
    ).toBe(false);
  });
});

describe('workflowBarValueFromCommandProgress', () => {
  it('maps normalized inner percent into the command sub-range', () => {
    const range = { start: 55, end: 70 };
    const state = { totalStages: 2, currentStage: 2 };
    const inner = 0;
    const normalized = normalizedWorkflowCommandPercent(state, inner);
    expect(normalized).toBe(50);
    expect(workflowBarValueFromCommandProgress(range, state, inner)).toBe(
      interpolateProgress(range, 50)
    );
  });
});

describe('updateWorkflowCommandProgressState', () => {
  it('tracks total_stages from started and stage_number from stage_started', () => {
    let s = updateWorkflowCommandProgressState(null, {
      event: 'started',
      data: { total_stages: 3 }
    });
    expect(s.totalStages).toBe(3);
    expect(s.currentStage).toBe(1);
    s = updateWorkflowCommandProgressState(s, {
      event: 'stage_started',
      data: { stage_number: 2 }
    });
    expect(s.currentStage).toBe(2);
  });
});

describe('commandProgressRange grading metrics', () => {
  it('treats stopped submissions as resumable rather than attention', () => {
    expect(stageLabel('stopped')).toBe('stopped');
    expect(stageProgressTone('stopped')).toBe('muted');
  });

  it('splits preliminary band using criterion and question counts', () => {
    const metrics = { criterionCount: 2, questionCount: 1 };
    const prelim = commandProgressRange('grading.score-preliminary', metrics);
    expect(prelim).not.toBeNull();
    expect(prelim!.start).toBe(70);
    expect(prelim!.end).toBeLessThan(99);
    expect(prelim!.end).toBeGreaterThan(70);
  });

  it('maps scans.canonicalize into the same stage band as canonicalization UI state', () => {
    expect(stageLabel('canonicalize')).toBe('canonicalizing');
    expect(commandWorkflowStage('scans.canonicalize')).toBe('canonicalize');
    expect(commandProgressRange('scans.canonicalize')).toEqual({ start: 12, end: 25 });
  });

  it('maps scans.pii into the dedicated prescreen band', () => {
    expect(stageLabel('pii')).toBe('screening PII');
    expect(commandWorkflowStage('scans.pii')).toBe('pii');
    expect(commandProgressRange('scans.pii')).toEqual({ start: 45, end: 55 });
  });
});

describe('moderation eligibility from answer state', () => {
  it('marks crop_failed answers as not moderation eligible', () => {
    const answer = {
      questionId: 'q1',
      questionNumber: 1,
      cropImagePath: null,
      manualGradingRequired: true,
      manualGradingReason: 'crop_failed',
      moderationEligible: false,
      parseStatus: 'blocked',
      gradingStatus: 'manual_required',
      reviewRequired: false,
      verified: false,
      stale: false,
      warnings: []
    };
    expect(answer.moderationEligible).toBe(false);
  });

  it('marks clean PII-prescreened answers as moderation eligible', () => {
    const answer = {
      questionId: 'q1',
      questionNumber: 1,
      cropImagePath: '/tmp/q1.png',
      manualGradingRequired: false,
      manualGradingReason: null,
      moderationEligible: true,
      parseStatus: 'not_started',
      gradingStatus: 'not_started',
      reviewRequired: false,
      verified: false,
      stale: false,
      warnings: []
    };
    expect(answer.moderationEligible).toBe(true);
  });

  it('marks PII-blocked answers as moderation eligible even though manual grading is required', () => {
    const answer = {
      questionId: 'q1',
      questionNumber: 1,
      cropImagePath: '/tmp/q1.png',
      manualGradingRequired: true,
      manualGradingReason: 'pii_detected',
      moderationEligible: true,
      parseStatus: 'blocked',
      gradingStatus: 'manual_required',
      reviewRequired: false,
      verified: false,
      stale: false,
      warnings: []
    };
    expect(answer.moderationEligible).toBe(true);
    expect(answer.manualGradingRequired).toBe(true);
  });
});
