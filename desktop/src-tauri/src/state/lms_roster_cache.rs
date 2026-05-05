// SPDX-License-Identifier: AGPL-3.0-only
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use tauri::Emitter;

use crate::errors::{HostError, HostResult};
use crate::lms::{self, ActiveLmsRosterLoader, LmsRosterRow};
use crate::models::{AppSettings, LmsRosterCacheSnapshot, LmsRosterCacheStatus, RuntimeJobEvent};
use crate::project_store;

use super::{app_jobs, AppState, AppStateInner};

#[cfg(test)]
const LMS_ROSTER_RETRY_DELAY: Duration = Duration::from_millis(25);
#[cfg(not(test))]
const LMS_ROSTER_RETRY_DELAY: Duration = Duration::from_secs(15);

const LMS_ROSTER_CACHE_COMMAND_NAME: &str = "lms.roster-cache";
const LMS_ROSTER_CACHE_EVENT_TYPE: &str = "lms_roster_cache_updated";

#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectLmsRosterCache {
    generation: u64,
    project_path: Option<PathBuf>,
    lms_provider: Option<String>,
    course_id: Option<String>,
    active_loader: Option<ActiveLmsRosterLoader>,
    status: LmsRosterCacheStatus,
    rows: Vec<LmsRosterRow>,
    last_error: Option<String>,
    idle_reason: Option<String>,
    active_task_generation: Option<u64>,
}

impl ProjectLmsRosterCache {
    pub(crate) fn snapshot(&self) -> LmsRosterCacheSnapshot {
        LmsRosterCacheSnapshot {
            status: self.status.clone(),
            project_path: self
                .project_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            lms_provider: self.lms_provider.clone(),
            course_id: self.course_id.clone(),
            rows: self.rows.clone(),
            last_error: self.last_error.clone(),
            idle_reason: self.idle_reason.clone(),
        }
    }

    fn same_loader_context(&self, project_path: &Path, loader: &ActiveLmsRosterLoader) -> bool {
        self.project_path.as_deref() == Some(project_path)
            && self.active_loader.as_ref() == Some(loader)
            && self.lms_provider.as_deref() == Some(loader.provider_id())
            && self.course_id.as_deref() == Some(loader.course_id())
    }

    fn set_idle(
        &mut self,
        project_path: Option<PathBuf>,
        lms_provider: Option<String>,
        course_id: Option<String>,
        reason: String,
    ) {
        self.generation = self.generation.wrapping_add(1);
        self.project_path = project_path;
        self.lms_provider = lms_provider;
        self.course_id = course_id;
        self.active_loader = None;
        self.status = LmsRosterCacheStatus::Idle;
        self.rows.clear();
        self.last_error = None;
        self.idle_reason = Some(reason);
        self.active_task_generation = None;
    }

    fn start_loading(&mut self, project_path: PathBuf, loader: ActiveLmsRosterLoader) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.project_path = Some(project_path);
        self.lms_provider = Some(loader.provider_id().to_string());
        self.course_id = Some(loader.course_id().to_string());
        self.active_loader = Some(loader);
        self.status = LmsRosterCacheStatus::Loading;
        self.rows.clear();
        self.last_error = None;
        self.idle_reason = None;
        self.active_task_generation = Some(self.generation);
        self.generation
    }

    fn mark_retry_loading(&mut self, generation: u64) -> bool {
        if self.generation != generation || self.active_task_generation != Some(generation) {
            return false;
        }
        self.status = LmsRosterCacheStatus::Loading;
        self.last_error = None;
        self.idle_reason = None;
        true
    }

    fn finish_ready(&mut self, generation: u64, rows: Vec<LmsRosterRow>) -> bool {
        if self.generation != generation || self.active_task_generation != Some(generation) {
            return false;
        }
        self.status = LmsRosterCacheStatus::Ready;
        self.rows = rows;
        self.last_error = None;
        self.idle_reason = None;
        self.active_task_generation = None;
        true
    }

    fn finish_error(&mut self, generation: u64, message: String) -> bool {
        if self.generation != generation || self.active_task_generation != Some(generation) {
            return false;
        }
        self.status = LmsRosterCacheStatus::Error;
        self.rows.clear();
        self.last_error = Some(message);
        self.idle_reason = None;
        true
    }
}

enum CacheResolution {
    Idle {
        project_path: Option<PathBuf>,
        provider: Option<String>,
        course_id: Option<String>,
        reason: String,
    },
    Ready {
        project_path: PathBuf,
        loader: ActiveLmsRosterLoader,
    },
}

fn current_project_config(project_path: &Path) -> HostResult<crate::models::ProjectConfig> {
    let connection = Connection::open(crate::project_store::schema::project_db_path(project_path))?;
    crate::project_store::schema::initialize_schema(&connection)?;
    project_store::load_project_config(&connection)
}

fn resolve_cache_context(
    state: &Arc<AppStateInner>,
    settings: &AppSettings,
) -> HostResult<CacheResolution> {
    let project_path = {
        let app = state.lock();
        app.current_project_path_optional()
    };
    let Some(project_path) = project_path else {
        return Ok(CacheResolution::Idle {
            project_path: None,
            provider: None,
            course_id: None,
            reason: "No project is currently open.".into(),
        });
    };

    let project_config = current_project_config(&project_path)?;
    match lms::resolve_active_roster_loader(&project_config, settings) {
        Ok(loader) => Ok(CacheResolution::Ready {
            project_path,
            loader,
        }),
        Err(idle) => Ok(CacheResolution::Idle {
            project_path: Some(project_path),
            provider: idle.provider,
            course_id: idle.course_id,
            reason: idle.reason,
        }),
    }
}

fn emit_cache_event(state: &Arc<AppStateInner>) {
    let (app_handle, worker_status, snapshot) = {
        let app = state.lock();
        let Some(app_handle) = app.app_handle.clone() else {
            return;
        };
        (
            app_handle,
            app_jobs::current_worker_status(&app),
            app.lms_roster_cache.snapshot(),
        )
    };
    let _ = app_handle.emit(
        super::RUNTIME_JOB_EVENT_NAME,
        RuntimeJobEvent {
            event_type: LMS_ROSTER_CACHE_EVENT_TYPE.into(),
            command_name: LMS_ROSTER_CACHE_COMMAND_NAME.into(),
            worker_status,
            request_id: None,
            job_id: None,
            payload: serde_json::to_value(snapshot).unwrap_or_default(),
        },
    );
}

async fn run_roster_preload_loop(
    state: Arc<AppStateInner>,
    loader: ActiveLmsRosterLoader,
    generation: u64,
) {
    loop {
        let fetch_result = lms::load_course_roster(&loader).await;
        match fetch_result {
            Ok(rows) => {
                let updated = {
                    let mut app = state.lock();
                    app.lms_roster_cache.finish_ready(generation, rows)
                };
                if updated {
                    emit_cache_event(&state);
                }
                return;
            }
            Err(err) => {
                let updated = {
                    let mut app = state.lock();
                    app.lms_roster_cache
                        .finish_error(generation, err.to_string())
                };
                if !updated {
                    return;
                }
                emit_cache_event(&state);
            }
        }

        let retry_delay = LMS_ROSTER_RETRY_DELAY;
        let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(retry_delay)).await;

        let should_retry = {
            let mut app = state.lock();
            app.lms_roster_cache.mark_retry_loading(generation)
        };
        if !should_retry {
            return;
        }
        emit_cache_event(&state);
    }
}

fn ensure_preload_for_loader(
    state: &Arc<AppStateInner>,
    project_path: PathBuf,
    loader: ActiveLmsRosterLoader,
) -> LmsRosterCacheSnapshot {
    let (snapshot, started) = {
        let mut app = state.lock();
        if app
            .lms_roster_cache
            .same_loader_context(&project_path, &loader)
            && (matches!(app.lms_roster_cache.status, LmsRosterCacheStatus::Ready)
                || app.lms_roster_cache.active_task_generation.is_some())
        {
            return app.lms_roster_cache.snapshot();
        }
        let generation = app
            .lms_roster_cache
            .start_loading(project_path, loader.clone());
        (app.lms_roster_cache.snapshot(), Some((loader, generation)))
    };
    emit_cache_event(state);
    if let Some((loader, generation)) = started {
        tauri::async_runtime::spawn(run_roster_preload_loop(
            Arc::clone(state),
            loader,
            generation,
        ));
    }
    snapshot
}

fn reconcile_cache_state(
    state: &Arc<AppStateInner>,
    settings: &AppSettings,
    start_loading: bool,
) -> HostResult<LmsRosterCacheSnapshot> {
    match resolve_cache_context(state, settings)? {
        CacheResolution::Idle {
            project_path,
            provider,
            course_id,
            reason,
        } => Ok(reconcile_idle_resolution(
            state,
            project_path,
            provider,
            course_id,
            reason,
        )),
        CacheResolution::Ready {
            project_path,
            loader,
        } => Ok(reconcile_ready_resolution(
            state,
            start_loading,
            project_path,
            loader,
        )),
    }
}

fn reconcile_idle_resolution(
    state: &Arc<AppStateInner>,
    project_path: Option<PathBuf>,
    provider: Option<String>,
    course_id: Option<String>,
    reason: String,
) -> LmsRosterCacheSnapshot {
    let (snapshot, changed) = {
        let mut app = state.lock();
        let unchanged = app.lms_roster_cache.status == LmsRosterCacheStatus::Idle
            && app.lms_roster_cache.project_path == project_path
            && app.lms_roster_cache.lms_provider == provider
            && app.lms_roster_cache.course_id == course_id
            && app.lms_roster_cache.idle_reason.as_deref() == Some(reason.as_str())
            && app.lms_roster_cache.rows.is_empty();
        if unchanged {
            (app.lms_roster_cache.snapshot(), false)
        } else {
            app.lms_roster_cache
                .set_idle(project_path, provider, course_id, reason);
            (app.lms_roster_cache.snapshot(), true)
        }
    };
    emit_cache_event_if_changed(state, changed);
    snapshot
}

fn reconcile_ready_resolution(
    state: &Arc<AppStateInner>,
    start_loading: bool,
    project_path: PathBuf,
    loader: ActiveLmsRosterLoader,
) -> LmsRosterCacheSnapshot {
    if start_loading {
        return ensure_preload_for_loader(state, project_path, loader);
    }

    let (snapshot, changed) = {
        let mut app = state.lock();
        if !app
            .lms_roster_cache
            .same_loader_context(&project_path, &loader)
        {
            app.lms_roster_cache.set_idle(
                Some(project_path),
                Some(loader.provider_id().to_string()),
                Some(loader.course_id().to_string()),
                "Shared LMS roster preload has not started yet.".into(),
            );
            (app.lms_roster_cache.snapshot(), true)
        } else {
            (app.lms_roster_cache.snapshot(), false)
        }
    };
    emit_cache_event_if_changed(state, changed);
    snapshot
}

fn emit_cache_event_if_changed(state: &Arc<AppStateInner>, changed: bool) {
    if changed {
        emit_cache_event(state);
    }
}

fn snapshot_for_consumer(
    state: &Arc<AppStateInner>,
    settings: &AppSettings,
) -> HostResult<LmsRosterCacheSnapshot> {
    reconcile_cache_state(state, settings, true)
}

pub(crate) fn clear_for_closed_project(state: &Arc<AppStateInner>) {
    {
        let mut app = state.lock();
        app.lms_roster_cache
            .set_idle(None, None, None, "No project is currently open.".into());
    }
    emit_cache_event(state);
}

pub(crate) fn ensure_project_preload(
    state: &Arc<AppStateInner>,
    settings: &AppSettings,
) -> HostResult<LmsRosterCacheSnapshot> {
    reconcile_cache_state(state, settings, true)
}

pub(crate) fn snapshot(
    app_state: &AppState,
    settings: &AppSettings,
) -> HostResult<LmsRosterCacheSnapshot> {
    reconcile_cache_state(&app_state.clone_inner(), settings, false)
}

pub(crate) fn ensure_preload(
    app_state: &AppState,
    settings: &AppSettings,
) -> HostResult<LmsRosterCacheSnapshot> {
    ensure_project_preload(&app_state.clone_inner(), settings)
}

pub(crate) fn required_cached_rows(
    app_state: &AppState,
    settings: &AppSettings,
    action: &str,
) -> HostResult<(String, Vec<LmsRosterRow>)> {
    let snapshot = snapshot_for_consumer(&app_state.clone_inner(), settings)?;
    match snapshot.status {
        LmsRosterCacheStatus::Ready => Ok((snapshot.course_id.unwrap_or_default(), snapshot.rows)),
        LmsRosterCacheStatus::Idle => Err(HostError::Validation(format!(
            "{action} requires a shared LMS roster, but preload is idle: {}",
            snapshot
                .idle_reason
                .unwrap_or_else(|| "missing roster prerequisites".into())
        ))),
        LmsRosterCacheStatus::Loading => Err(HostError::Conflict(format!(
            "{action} requires the shared LMS roster, but preload is still running."
        ))),
        LmsRosterCacheStatus::Error => Err(HostError::Conflict(format!(
            "{action} requires the shared LMS roster, but preload failed: {}",
            snapshot
                .last_error
                .unwrap_or_else(|| "unknown LMS roster preload error".into())
        ))),
    }
}

pub(crate) fn cached_rows_if_ready(
    state: &Arc<AppStateInner>,
    settings: &AppSettings,
) -> Option<(String, Vec<LmsRosterRow>)> {
    let snapshot = snapshot_for_consumer(state, settings).ok()?;
    (snapshot.status == LmsRosterCacheStatus::Ready)
        .then_some((snapshot.course_id.unwrap_or_default(), snapshot.rows))
}
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use crate::lms::{ActiveLmsRosterLoader, LmsRosterRow, __test_set_roster_fetch_override};
    use crate::models::{AppSettings, InstructorProfile, LmsRosterCacheStatus};
    use crate::state::AppState;
    use crate::test_support::{lock_env_vars, EnvVarGuard};

    struct OverrideGuard;

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            __test_set_roster_fetch_override(None);
        }
    }

    fn canvas_settings() -> AppSettings {
        AppSettings {
            lms_provider: "canvas".into(),
            lms_canvas_base_url: "https://canvas.example.test".into(),
            lms_canvas_api_key: Some("token".into()),
            ..AppSettings::default()
        }
    }

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    fn create_project_with_course(course_id: &str) -> PathBuf {
        let created = crate::project_store::create_project(
            &format!("Roster Cache {course_id}"),
            None,
            None,
            Some(course_id.into()),
            &InstructorProfile::default(),
        )
        .expect("project should be created");
        PathBuf::from(created.project_path)
    }

    fn wait_for_ready(
        state: &AppState,
        settings: &AppSettings,
    ) -> crate::models::LmsRosterCacheSnapshot {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = state
                .lms_roster_cache_snapshot(settings)
                .expect("cache snapshot should load");
            if snapshot.status == LmsRosterCacheStatus::Ready {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for ready roster cache; last snapshot: {:?}",
                snapshot.status
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn set_blocked_canvas_override(gate: Arc<(Mutex<bool>, Condvar)>) {
        __test_set_roster_fetch_override(Some(Arc::new(move |loader| match loader {
            ActiveLmsRosterLoader::Canvas { course_id, .. } => {
                wait_for_gate_if_course_matches(&gate, course_id, "course-a");
                Ok(canvas_test_rows(course_id))
            }
        })));
    }

    fn wait_for_gate_if_course_matches(
        gate: &Arc<(Mutex<bool>, Condvar)>,
        course_id: &str,
        blocked_course_id: &str,
    ) {
        if course_id != blocked_course_id {
            return;
        }
        let (lock, cvar) = &**gate;
        let mut released = lock.lock().expect("gate lock");
        while !*released {
            released = cvar.wait(released).expect("gate wait");
        }
    }

    fn canvas_test_rows(course_id: &str) -> Vec<LmsRosterRow> {
        vec![LmsRosterRow {
            user_id: format!("{course_id}-student"),
            display_name: format!("Student for {course_id}"),
            sort_key: format!("{course_id}, student"),
            email: None,
            login_id: None,
        }]
    }

    fn release_gate(gate: &Arc<(Mutex<bool>, Condvar)>) {
        let (lock, cvar) = &**gate;
        *lock.lock().expect("gate lock") = true;
        cvar.notify_all();
    }

    #[test]
    fn open_project_preloads_and_reuses_cached_rows() {
        let _guard = lock_env_vars();
        let _override_guard = OverrideGuard;
        let test_root = temp_root("scriptscore-roster-cache-open");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let project_path = create_project_with_course("course-a");
        let fetch_count = Arc::new(AtomicUsize::new(0));
        let fetch_count_for_override = Arc::clone(&fetch_count);
        __test_set_roster_fetch_override(Some(Arc::new(move |loader| {
            fetch_count_for_override.fetch_add(1, Ordering::SeqCst);
            match loader {
                ActiveLmsRosterLoader::Canvas { course_id, .. } => Ok(vec![LmsRosterRow {
                    user_id: format!("{course_id}-student"),
                    display_name: "Test Student".into(),
                    sort_key: "student, test".into(),
                    email: None,
                    login_id: None,
                }]),
            }
        })));

        let state = AppState::bootstrap();
        let settings = canvas_settings();
        state
            .open_project(project_path, &settings)
            .expect("project should open");

        let snapshot = wait_for_ready(&state, &settings);
        assert_eq!(snapshot.course_id.as_deref(), Some("course-a"));
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(fetch_count.load(Ordering::SeqCst), 1);

        let (_, rows) = super::required_cached_rows(&state, &settings, "Test action")
            .expect("ready cache should satisfy consumer reads");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            fetch_count.load(Ordering::SeqCst),
            1,
            "reading ready cache must not refetch roster"
        );
    }

    #[test]
    fn close_project_clears_cached_rows() {
        let _guard = lock_env_vars();
        let _override_guard = OverrideGuard;
        let test_root = temp_root("scriptscore-roster-cache-close");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let project_path = create_project_with_course("course-close");
        __test_set_roster_fetch_override(Some(Arc::new(|_| {
            Ok(vec![LmsRosterRow {
                user_id: "student-1".into(),
                display_name: "Student One".into(),
                sort_key: "one, student".into(),
                email: None,
                login_id: None,
            }])
        })));

        let state = AppState::bootstrap();
        let settings = canvas_settings();
        state
            .open_project(project_path, &settings)
            .expect("project should open");
        let _snapshot = wait_for_ready(&state, &settings);

        state.close_current_project().expect("project should close");
        let snapshot = state
            .lms_roster_cache_snapshot(&settings)
            .expect("closed project snapshot should load");
        assert_eq!(snapshot.status, LmsRosterCacheStatus::Idle);
        assert!(snapshot.rows.is_empty());
        assert!(snapshot.project_path.is_none());
    }

    #[test]
    fn preload_retries_until_success() {
        let _guard = lock_env_vars();
        let _override_guard = OverrideGuard;
        let test_root = temp_root("scriptscore-roster-cache-retry");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let project_path = create_project_with_course("course-retry");
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_override = Arc::clone(&attempts);
        __test_set_roster_fetch_override(Some(Arc::new(move |_| {
            let attempt = attempts_for_override.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                return Err(crate::errors::HostError::Project(
                    "temporary roster outage".into(),
                ));
            }
            Ok(vec![LmsRosterRow {
                user_id: "student-2".into(),
                display_name: "Student Two".into(),
                sort_key: "two, student".into(),
                email: None,
                login_id: None,
            }])
        })));

        let state = AppState::bootstrap();
        let settings = canvas_settings();
        state
            .open_project(project_path, &settings)
            .expect("project should open");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_error = false;
        while Instant::now() < deadline {
            let snapshot = state
                .lms_roster_cache_snapshot(&settings)
                .expect("cache snapshot should load");
            if snapshot.status == LmsRosterCacheStatus::Error {
                saw_error = true;
            }
            if snapshot.status == LmsRosterCacheStatus::Ready {
                assert!(
                    saw_error,
                    "retry path should expose error before succeeding"
                );
                assert_eq!(attempts.load(Ordering::SeqCst), 2);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for retrying roster cache to become ready");
    }

    #[test]
    fn switching_projects_ignores_stale_async_completion() {
        let _guard = lock_env_vars();
        let _override_guard = OverrideGuard;
        let test_root = temp_root("scriptscore-roster-cache-stale");
        let _projects_root = EnvVarGuard::set("SCRIPTSCORE_PROJECTS_ROOT", &test_root);
        let project_a = create_project_with_course("course-a");
        let project_b = create_project_with_course("course-b");
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        set_blocked_canvas_override(Arc::clone(&gate));

        let state = AppState::bootstrap();
        let settings = canvas_settings();
        state
            .open_project(project_a, &settings)
            .expect("project A should open");
        state
            .open_project(project_b, &settings)
            .expect("project B should open");

        let snapshot = wait_for_ready(&state, &settings);
        assert_eq!(snapshot.course_id.as_deref(), Some("course-b"));
        assert_eq!(snapshot.rows[0].user_id, "course-b-student");

        release_gate(&gate);
        std::thread::sleep(Duration::from_millis(50));

        let snapshot = state
            .lms_roster_cache_snapshot(&settings)
            .expect("cache snapshot should load");
        assert_eq!(snapshot.course_id.as_deref(), Some("course-b"));
        assert_eq!(snapshot.rows[0].user_id, "course-b-student");
    }
}
