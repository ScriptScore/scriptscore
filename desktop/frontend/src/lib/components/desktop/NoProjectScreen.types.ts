// SPDX-License-Identifier: AGPL-3.0-only
export interface RecentProject {
  projectPath: string;
  displayName: string;
  courseCode: string | null;
  openedAt: string;
}

export interface NoProjectScreenProps {
  hasDesktopHost?: boolean;
  showCreateForm?: boolean;
  busyAction?: import('$lib/stores/workspaceView').BusyAction;
  createInput: import('$lib/types').CreateProjectInput;
  actionError?: string | null;
  createProgress?: number | null;
  recentProjects?: RecentProject[];
  forceOnboardingOpen?: boolean;
  onShowCreateForm?: (() => void) | null;
  onHideCreateForm?: (() => void) | null;
  onOpenProject?: (() => void | Promise<void>) | null;
  onOpenRecentProject?: ((projectPath: string) => void | Promise<void>) | null;
  onChooseTemplatePdfForCreate?: (() => void | Promise<void>) | null;
  onSubmitCreate?: (() => void | Promise<void>) | null;
  onCloseOnboarding?: (() => void) | null;
  onOpenSettings?: (() => void) | null;
}
