// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Matches Setup workspace "Continue" readiness: either redaction is not required,
 * or at least one redaction region exists.
 */
export function isTemplateRedactionsReadyForReview(
  redactionRequired: boolean,
  redactionRegionCount: number
): boolean {
  return !redactionRequired || redactionRegionCount > 0;
}
