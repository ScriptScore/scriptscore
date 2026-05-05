// SPDX-License-Identifier: AGPL-3.0-only
export type FeedbackTone = 'success' | 'info' | 'warning' | 'error' | 'muted';

export function feedbackToneClass(tone: FeedbackTone): string {
  if (tone === 'success') {
    return 'border-message-success-border bg-message-success-bg text-message-success-text';
  }
  if (tone === 'warning') {
    return 'border-message-warning-border bg-message-warning-bg text-message-warning-text';
  }
  if (tone === 'error') {
    return 'border-message-error-border bg-message-error-bg text-message-error-text';
  }
  if (tone === 'muted') {
    return 'border-border-default bg-surface-message text-text-secondary';
  }
  return 'border-message-info-border bg-message-info-bg text-message-info-text';
}

export function feedbackRole(tone: FeedbackTone): 'status' | 'alert' {
  return tone === 'warning' || tone === 'error' ? 'alert' : 'status';
}
