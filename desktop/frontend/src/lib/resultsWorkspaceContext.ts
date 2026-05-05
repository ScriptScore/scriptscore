// SPDX-License-Identifier: AGPL-3.0-only
import type {
  AppSettings,
  ExamWorkspaceState,
  LmsRosterCacheSnapshot
} from '$lib/types';

const MAX_STUDENT_DISPLAY_NAME_CACHE_ENTRIES = 6;
const tokenNameCacheBySnapshotKey = new Map<string, ReadonlyMap<string, string>>();

export interface StudentDisplayNameResolverDeps {
  getLmsRosterCacheState: () => Promise<LmsRosterCacheSnapshot>;
  computeLmsBindingToken: (courseId: string, userId: string) => Promise<string>;
}

export function currentResultsAssignmentContextKey(args: {
  hasDesktopHost: boolean;
  currentProjectPath: string | null;
  settings: Pick<AppSettings, 'lmsProvider' | 'lmsCanvasBaseUrl'>;
  workspace: ExamWorkspaceState | null;
}): string | null {
  if (!args.hasDesktopHost || !args.currentProjectPath || !args.workspace) {
    return null;
  }
  const courseId =
    args.workspace.projectConfig?.lmsCourseId ??
    args.workspace.project.lmsCourseId ??
    '';
  if (!courseId.trim()) {
    return null;
  }
  return [
    args.currentProjectPath,
    args.settings.lmsProvider,
    args.settings.lmsCanvasBaseUrl,
    courseId
  ].join('::');
}

export async function resolveStudentDisplayNamesForWorkspace(
  workspace: ExamWorkspaceState,
  deps: StudentDisplayNameResolverDeps
): Promise<Record<string, string>> {
  const courseId =
    (workspace.projectConfig?.lmsCourseId ?? workspace.project.lmsCourseId ?? '').trim() || null;
  const persistedRoster = workspace.studentRoster ?? [];
  if (!courseId) {
    return Object.fromEntries(
      (workspace.studentIntake?.items ?? [])
        .map((item) => {
          const displayName = item.localDisplayName?.trim() ?? '';
          return displayName.length > 0 ? ([item.studentRef, displayName] as const) : null;
        })
        .filter((entry): entry is readonly [string, string] => entry !== null)
    );
  }
  if (persistedRoster.length === 0) {
    return {};
  }

  const snapshot = await deps.getLmsRosterCacheState();
  if (
    snapshot.status !== 'ready' ||
    snapshot.projectPath !== workspace.project.projectPath ||
    snapshot.courseId !== courseId
  ) {
    return {};
  }

  const tokenToDisplayName = await tokenToDisplayNameMap(snapshot, courseId, deps);
  return Object.fromEntries(
    persistedRoster
      .map((student) => {
        const displayName = tokenToDisplayName.get(student.bindingTokenHex)?.trim() ?? '';
        return displayName.length > 0 ? ([student.studentRef, displayName] as const) : null;
      })
      .filter((entry): entry is readonly [string, string] => entry !== null)
  );
}

async function tokenToDisplayNameMap(
  snapshot: LmsRosterCacheSnapshot,
  courseId: string,
  deps: StudentDisplayNameResolverDeps
): Promise<ReadonlyMap<string, string>> {
  const cacheKey = studentDisplayNameCacheKey(snapshot);
  const cached = tokenNameCacheBySnapshotKey.get(cacheKey);
  if (cached) {
    tokenNameCacheBySnapshotKey.delete(cacheKey);
    tokenNameCacheBySnapshotKey.set(cacheKey, cached);
    return cached;
  }

  const entries = await Promise.all(
    snapshot.rows.map(async (row) => {
      const displayName = row.displayName.trim();
      if (displayName.length === 0) {
        return null;
      }
      try {
        const bindingTokenHex = await deps.computeLmsBindingToken(courseId, row.userId);
        return bindingTokenHex.trim().length > 0
          ? ([bindingTokenHex, displayName] as const)
          : null;
      } catch {
        return null;
      }
    })
  );

  const tokenToDisplayName = new Map(
    entries.filter((entry): entry is readonly [string, string] => entry !== null)
  );
  tokenNameCacheBySnapshotKey.set(cacheKey, tokenToDisplayName);
  trimStudentDisplayNameCache();
  return tokenToDisplayName;
}

function studentDisplayNameCacheKey(snapshot: LmsRosterCacheSnapshot): string {
  return [
    snapshot.projectPath ?? '',
    snapshot.lmsProvider ?? '',
    snapshot.courseId ?? '',
    ...snapshot.rows.map((row) => `${row.userId}\u001f${row.displayName}`)
  ].join('\u001e');
}

function trimStudentDisplayNameCache() {
  while (tokenNameCacheBySnapshotKey.size > MAX_STUDENT_DISPLAY_NAME_CACHE_ENTRIES) {
    const oldestKey = tokenNameCacheBySnapshotKey.keys().next().value;
    if (!oldestKey) {
      break;
    }
    tokenNameCacheBySnapshotKey.delete(oldestKey);
  }
}

export function resetStudentDisplayNameCacheForTests() {
  tokenNameCacheBySnapshotKey.clear();
}
