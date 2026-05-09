// SPDX-License-Identifier: AGPL-3.0-only
const compactTabActionButtonBaseClass =
  'relative z-10 rounded-md px-2 py-0.5 text-[10px] font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-50';

export const compactTabActionButtonClass = `${compactTabActionButtonBaseClass} bg-interaction-active text-text-primary hover:bg-interaction-hover`;
export const compactWorkspaceTabActionButtonClass = `${compactTabActionButtonBaseClass} bg-workspace-sidebar-active text-workspace-text-primary hover:bg-workspace-sidebar-hover`;
