// SPDX-License-Identifier: AGPL-3.0-only
//! Pluggable LMS integrations (Canvas first).

pub mod canvas;

use serde::{Deserialize, Serialize};

use crate::errors::HostResult;
use crate::models::{
    AppSettings, LmsAssignmentSummary, LmsUploadMode, LmsUploadPreparationRow,
    LmsUploadPublishOutcome, ProjectConfig,
};
use crate::state::RuntimeEventSink;

/// Normalized course row for UI pickers.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsCourseSummary {
    pub lms_course_id: String,
    pub name: String,
    pub course_code: Option<String>,
}

/// One roster row for transient UI (names are not persisted in project DB).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmsRosterRow {
    /// Canvas user id as a string for stable binding inputs.
    pub user_id: String,
    pub display_name: String,
    /// Lowercase sort key (typically Canvas `sortable_name`).
    pub sort_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActiveLmsAssignmentLoader {
    Canvas {
        course_id: String,
        base_url: String,
        access_token: String,
    },
}

impl ActiveLmsAssignmentLoader {
    pub fn provider_id(&self) -> &'static str {
        match self {
            Self::Canvas { .. } => "canvas",
        }
    }

    pub fn course_id(&self) -> &str {
        match self {
            Self::Canvas { course_id, .. } => course_id.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActiveLmsResultsPublisher {
    Canvas {
        course_id: String,
        base_url: String,
        access_token: String,
        assignment_id: String,
    },
}

impl ActiveLmsResultsPublisher {
    pub fn provider_id(&self) -> &'static str {
        match self {
            Self::Canvas { .. } => "canvas",
        }
    }

    pub fn course_id(&self) -> &str {
        match self {
            Self::Canvas { course_id, .. } => course_id.as_str(),
        }
    }

    pub fn assignment_id(&self) -> &str {
        match self {
            Self::Canvas { assignment_id, .. } => assignment_id.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActiveLmsRosterLoader {
    Canvas {
        course_id: String,
        base_url: String,
        access_token: String,
    },
}

impl ActiveLmsRosterLoader {
    pub fn provider_id(&self) -> &'static str {
        match self {
            Self::Canvas { .. } => "canvas",
        }
    }

    pub fn course_id(&self) -> &str {
        match self {
            Self::Canvas { course_id, .. } => course_id.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LmsRosterIdleState {
    pub provider: Option<String>,
    pub course_id: Option<String>,
    pub reason: String,
}

pub type LmsResultsIdleState = LmsRosterIdleState;

fn current_provider(settings: &AppSettings) -> (String, Option<String>) {
    let provider = settings.lms_provider.trim().to_ascii_lowercase();
    let provider_opt = (!provider.is_empty()).then_some(provider.clone());
    (provider, provider_opt)
}

fn current_course_id(project_config: &ProjectConfig) -> Option<String> {
    project_config
        .lms_course_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn resolve_active_roster_loader(
    project_config: &ProjectConfig,
    settings: &AppSettings,
) -> Result<ActiveLmsRosterLoader, LmsRosterIdleState> {
    let (provider, provider_opt) = current_provider(settings);
    let course_id = current_course_id(project_config);

    let Some(course_id) = course_id else {
        return Err(LmsRosterIdleState {
            provider: provider_opt,
            course_id: None,
            reason: "Link an LMS course in Template setup before loading the roster.".into(),
        });
    };

    match provider.as_str() {
        "" | "none" => Err(LmsRosterIdleState {
            provider: provider_opt,
            course_id: Some(course_id),
            reason: "Choose an LMS provider in Settings before loading the roster.".into(),
        }),
        "canvas" => {
            let base_url = settings.lms_canvas_base_url.trim();
            let access_token = settings.lms_canvas_api_key.as_deref().unwrap_or("").trim();
            if base_url.is_empty() || access_token.is_empty() {
                return Err(LmsRosterIdleState {
                    provider: Some("canvas".into()),
                    course_id: Some(course_id),
                    reason: "Canvas LMS settings are incomplete. Add the base URL and access token in Settings.".into(),
                });
            }
            Ok(ActiveLmsRosterLoader::Canvas {
                course_id,
                base_url: base_url.to_string(),
                access_token: access_token.to_string(),
            })
        }
        _ => Err(LmsRosterIdleState {
            provider: provider_opt,
            course_id: Some(course_id),
            reason: format!(
                "The '{}' LMS provider is not supported for roster loading.",
                settings.lms_provider.trim()
            ),
        }),
    }
}

pub fn resolve_active_assignment_loader(
    project_config: &ProjectConfig,
    settings: &AppSettings,
) -> Result<ActiveLmsAssignmentLoader, LmsResultsIdleState> {
    let (provider, provider_opt) = current_provider(settings);
    let course_id = current_course_id(project_config);

    let Some(course_id) = course_id else {
        return Err(LmsResultsIdleState {
            provider: provider_opt,
            course_id: None,
            reason: "Link an LMS course in Template setup before selecting an assignment.".into(),
        });
    };

    match provider.as_str() {
        "" | "none" => Err(LmsResultsIdleState {
            provider: provider_opt,
            course_id: Some(course_id),
            reason: "Choose an LMS provider in Settings before selecting an assignment.".into(),
        }),
        "canvas" => {
            let base_url = settings.lms_canvas_base_url.trim();
            let access_token = settings.lms_canvas_api_key.as_deref().unwrap_or("").trim();
            if base_url.is_empty() || access_token.is_empty() {
                return Err(LmsResultsIdleState {
                    provider: Some("canvas".into()),
                    course_id: Some(course_id),
                    reason: "Canvas LMS settings are incomplete. Add the base URL and access token in Settings.".into(),
                });
            }
            Ok(ActiveLmsAssignmentLoader::Canvas {
                course_id,
                base_url: base_url.to_string(),
                access_token: access_token.to_string(),
            })
        }
        _ => Err(LmsResultsIdleState {
            provider: provider_opt,
            course_id: Some(course_id),
            reason: format!(
                "The '{}' LMS provider is not supported for assignment loading.",
                settings.lms_provider.trim()
            ),
        }),
    }
}

pub fn resolve_active_results_publisher(
    project_config: &ProjectConfig,
    settings: &AppSettings,
) -> Result<ActiveLmsResultsPublisher, LmsResultsIdleState> {
    let assignment_id = project_config
        .lms_assignment_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let Some(assignment_id) = assignment_id else {
        return Err(LmsResultsIdleState {
            provider: None,
            course_id: current_course_id(project_config),
            reason: "Choose an LMS assignment before running upload.".into(),
        });
    };

    match resolve_active_assignment_loader(project_config, settings)? {
        ActiveLmsAssignmentLoader::Canvas {
            course_id,
            base_url,
            access_token,
        } => Ok(ActiveLmsResultsPublisher::Canvas {
            course_id,
            base_url,
            access_token,
            assignment_id,
        }),
    }
}

#[cfg(test)]
type TestRosterFetcher =
    std::sync::Arc<dyn Fn(&ActiveLmsRosterLoader) -> HostResult<Vec<LmsRosterRow>> + Send + Sync>;

#[cfg(test)]
type TestRosterFetcherSlot = Option<TestRosterFetcher>;

#[cfg(test)]
fn test_roster_fetch_override() -> std::sync::MutexGuard<'static, TestRosterFetcherSlot> {
    static OVERRIDE: std::sync::OnceLock<std::sync::Mutex<TestRosterFetcherSlot>> =
        std::sync::OnceLock::new();
    OVERRIDE
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("test roster fetch override lock")
}

#[cfg(test)]
pub fn __test_set_roster_fetch_override(fetcher: TestRosterFetcherSlot) {
    *test_roster_fetch_override() = fetcher;
}

pub async fn load_course_roster(loader: &ActiveLmsRosterLoader) -> HostResult<Vec<LmsRosterRow>> {
    #[cfg(test)]
    {
        let fetcher = {
            let guard = test_roster_fetch_override();
            guard.as_ref().cloned()
        };
        if let Some(fetcher) = fetcher {
            let loader = loader.clone();
            return tauri::async_runtime::spawn_blocking(move || fetcher(&loader))
                .await
                .map_err(|err| {
                    crate::errors::HostError::Project(format!(
                        "test LMS roster override panicked: {err}"
                    ))
                })?;
        }
    }

    match loader {
        ActiveLmsRosterLoader::Canvas {
            course_id,
            base_url,
            access_token,
        } => canvas::list_course_roster(base_url, access_token, course_id).await,
    }
}

#[cfg(test)]
type TestAssignmentsFetcher = std::sync::Arc<
    dyn Fn(&ActiveLmsAssignmentLoader) -> HostResult<Vec<LmsAssignmentSummary>> + Send + Sync,
>;

#[cfg(test)]
type TestAssignmentsFetcherSlot = Option<TestAssignmentsFetcher>;

#[cfg(test)]
fn test_assignments_fetch_override() -> std::sync::MutexGuard<'static, TestAssignmentsFetcherSlot> {
    static OVERRIDE: std::sync::OnceLock<std::sync::Mutex<TestAssignmentsFetcherSlot>> =
        std::sync::OnceLock::new();
    OVERRIDE
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("test assignments fetch override lock")
}

#[cfg(test)]
pub fn __test_set_assignments_fetch_override(fetcher: TestAssignmentsFetcherSlot) {
    *test_assignments_fetch_override() = fetcher;
}

pub async fn load_course_assignments(
    loader: &ActiveLmsAssignmentLoader,
) -> HostResult<Vec<LmsAssignmentSummary>> {
    #[cfg(test)]
    {
        let fetcher = {
            let guard = test_assignments_fetch_override();
            guard.as_ref().cloned()
        };
        if let Some(fetcher) = fetcher {
            let loader = loader.clone();
            return tauri::async_runtime::spawn_blocking(move || fetcher(&loader))
                .await
                .map_err(|err| {
                    crate::errors::HostError::Project(format!(
                        "test LMS assignments override panicked: {err}"
                    ))
                })?;
        }
    }

    match loader {
        ActiveLmsAssignmentLoader::Canvas {
            course_id,
            base_url,
            access_token,
        } => canvas::list_course_assignments(base_url, access_token, course_id).await,
    }
}

#[cfg(test)]
type TestResultsPublisher = std::sync::Arc<
    dyn Fn(
            &ActiveLmsResultsPublisher,
            LmsUploadMode,
            &[LmsUploadPreparationRow],
        ) -> HostResult<Vec<LmsUploadPublishOutcome>>
        + Send
        + Sync,
>;

#[cfg(test)]
type TestResultsPublisherSlot = Option<TestResultsPublisher>;

#[cfg(test)]
fn test_results_publish_override() -> std::sync::MutexGuard<'static, TestResultsPublisherSlot> {
    static OVERRIDE: std::sync::OnceLock<std::sync::Mutex<TestResultsPublisherSlot>> =
        std::sync::OnceLock::new();
    OVERRIDE
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("test results publish override lock")
}

#[cfg(test)]
pub fn __test_set_results_publish_override(publisher: TestResultsPublisherSlot) {
    *test_results_publish_override() = publisher;
}

pub async fn publish_results(
    publisher: &ActiveLmsResultsPublisher,
    mode: LmsUploadMode,
    rows: &[LmsUploadPreparationRow],
    event_sink: &dyn RuntimeEventSink,
    batch_id: &str,
) -> HostResult<Vec<LmsUploadPublishOutcome>> {
    #[cfg(test)]
    {
        let override_fn = {
            let guard = test_results_publish_override();
            guard.as_ref().cloned()
        };
        if let Some(override_fn) = override_fn {
            let publisher = publisher.clone();
            let rows = rows.to_vec();
            return tauri::async_runtime::spawn_blocking(move || {
                override_fn(&publisher, mode, &rows)
            })
            .await
            .map_err(|err| {
                crate::errors::HostError::Project(format!(
                    "test LMS publish override panicked: {err}"
                ))
            })?;
        }
    }

    match publisher {
        ActiveLmsResultsPublisher::Canvas {
            course_id,
            base_url,
            access_token,
            assignment_id,
        } => {
            canvas::publish_assignment_results(
                canvas::CanvasResultsPublisherConfig {
                    base_url,
                    access_token,
                    course_id,
                    assignment_id,
                },
                mode,
                rows,
                canvas::CanvasUploadProgress {
                    event_sink,
                    batch_id,
                },
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_config_with_course(course_id: Option<&str>) -> ProjectConfig {
        ProjectConfig {
            lms_course_id: course_id.map(str::to_string),
            ..ProjectConfig::default()
        }
    }

    #[test]
    fn resolve_active_roster_loader_requires_course_link() {
        let settings = AppSettings {
            lms_provider: "canvas".into(),
            lms_canvas_base_url: "https://canvas.example.test".into(),
            lms_canvas_api_key: Some("token".into()),
            ..AppSettings::default()
        };
        let error = resolve_active_roster_loader(&project_config_with_course(None), &settings)
            .expect_err("missing course should stay idle");
        assert_eq!(error.provider.as_deref(), Some("canvas"));
        assert!(error.reason.contains("Link an LMS course"));
    }

    #[test]
    fn resolve_active_roster_loader_dispatches_canvas() {
        let settings = AppSettings {
            lms_provider: "canvas".into(),
            lms_canvas_base_url: "https://canvas.example.test".into(),
            lms_canvas_api_key: Some("token".into()),
            ..AppSettings::default()
        };
        let loader =
            resolve_active_roster_loader(&project_config_with_course(Some("course-42")), &settings)
                .expect("canvas loader should resolve");
        match loader {
            ActiveLmsRosterLoader::Canvas {
                course_id,
                base_url,
                access_token,
            } => {
                assert_eq!(course_id, "course-42");
                assert_eq!(base_url, "https://canvas.example.test");
                assert_eq!(access_token, "token");
            }
        }
    }

    #[test]
    fn resolve_active_assignment_loader_requires_course_link() {
        let settings = AppSettings {
            lms_provider: "canvas".into(),
            lms_canvas_base_url: "https://canvas.example.test".into(),
            lms_canvas_api_key: Some("token".into()),
            ..AppSettings::default()
        };
        let error = resolve_active_assignment_loader(&project_config_with_course(None), &settings)
            .expect_err("missing course should stay idle");
        assert!(error.reason.contains("Link an LMS course"));
    }

    #[test]
    fn resolve_active_results_publisher_requires_assignment_id() {
        let settings = AppSettings {
            lms_provider: "canvas".into(),
            lms_canvas_base_url: "https://canvas.example.test".into(),
            lms_canvas_api_key: Some("token".into()),
            ..AppSettings::default()
        };
        let error = resolve_active_results_publisher(
            &ProjectConfig {
                lms_course_id: Some("course-42".into()),
                ..ProjectConfig::default()
            },
            &settings,
        )
        .expect_err("missing assignment should stay idle");
        assert!(error.reason.contains("Choose an LMS assignment"));
    }
}
