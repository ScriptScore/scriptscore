// SPDX-License-Identifier: AGPL-3.0-only
use std::time::Duration;

use reqwest::header::{ACCEPT, USER_AGENT};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const LATEST_STABLE_RELEASE_URL: &str =
    "https://api.github.com/repos/ScriptScore/scriptscore/releases/latest";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateStatus {
    UpToDate,
    UpdateAvailable,
    NoStableRelease,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheck {
    pub installed_version: String,
    pub latest_stable_version: Option<String>,
    pub latest_stable_tag: Option<String>,
    pub release_url: Option<String>,
    pub update_available: bool,
    pub status: AppUpdateStatus,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<AppUpdateCheck, String> {
    let installed_version = app.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| err.to_string())?;

    let response = match client
        .get(LATEST_STABLE_RELEASE_URL)
        .header(USER_AGENT, "ScriptScore Desktop")
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return Ok(unavailable(
                installed_version,
                format!("Could not check for updates: {err}"),
            ))
        }
    };

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(AppUpdateCheck {
            installed_version,
            latest_stable_version: None,
            latest_stable_tag: None,
            release_url: None,
            update_available: false,
            status: AppUpdateStatus::NoStableRelease,
            message: "No stable release has been published yet.".into(),
        });
    }

    if !response.status().is_success() {
        return Ok(unavailable(
            installed_version,
            format!(
                "Could not check for updates: GitHub returned {}.",
                response.status()
            ),
        ));
    }

    match response.json::<GitHubRelease>().await {
        Ok(release) => Ok(update_check_from_release(&installed_version, release)),
        Err(err) => Ok(unavailable(
            installed_version,
            format!("Could not read GitHub release metadata: {err}"),
        )),
    }
}

fn update_check_from_release(installed_version: &str, release: GitHubRelease) -> AppUpdateCheck {
    if release.draft || release.prerelease {
        return unavailable(
            installed_version.to_string(),
            "GitHub latest release metadata did not point to a stable release.".into(),
        );
    }

    let Some(latest_version) = stable_version_from_tag(&release.tag_name) else {
        return unavailable(
            installed_version.to_string(),
            "GitHub latest release tag is not a stable semantic version.".into(),
        );
    };

    let Ok(installed) = Version::parse(installed_version.trim()) else {
        return unavailable(
            installed_version.to_string(),
            "Installed app version is not a semantic version.".into(),
        );
    };

    let latest_stable_version = latest_version.to_string();
    if latest_version > installed {
        AppUpdateCheck {
            installed_version: installed_version.to_string(),
            latest_stable_version: Some(latest_stable_version),
            latest_stable_tag: Some(release.tag_name),
            release_url: Some(release.html_url),
            update_available: true,
            status: AppUpdateStatus::UpdateAvailable,
            message: "A newer stable ScriptScore Desktop release is available.".into(),
        }
    } else {
        AppUpdateCheck {
            installed_version: installed_version.to_string(),
            latest_stable_version: Some(latest_stable_version),
            latest_stable_tag: Some(release.tag_name),
            release_url: Some(release.html_url),
            update_available: false,
            status: AppUpdateStatus::UpToDate,
            message: "You have the latest stable ScriptScore Desktop release.".into(),
        }
    }
}

fn stable_version_from_tag(tag: &str) -> Option<Version> {
    let normalized = tag.trim().strip_prefix('v').unwrap_or_else(|| tag.trim());
    let version = Version::parse(normalized).ok()?;
    if version.pre.is_empty() {
        Some(version)
    } else {
        None
    }
}

fn unavailable(installed_version: String, message: String) -> AppUpdateCheck {
    AppUpdateCheck {
        installed_version,
        latest_stable_version: None,
        latest_stable_tag: None,
        release_url: None,
        update_available: false,
        status: AppUpdateStatus::Unavailable,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag_name: &str) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag_name.into(),
            html_url: format!("https://github.com/ScriptScore/scriptscore/releases/tag/{tag_name}"),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn update_stable_tag_parses_plain_and_v_prefixed_versions() {
        assert_eq!(
            stable_version_from_tag("0.1.0").unwrap(),
            Version::parse("0.1.0").unwrap()
        );
        assert_eq!(
            stable_version_from_tag("v0.1.0").unwrap(),
            Version::parse("0.1.0").unwrap()
        );
    }

    #[test]
    fn update_stable_tag_rejects_rc_and_prefixed_prerelease_tags() {
        assert!(stable_version_from_tag("v0.1.0-rc.1").is_none());
        assert!(stable_version_from_tag("0.1.0-beta.1").is_none());
        assert!(stable_version_from_tag("rc-0.1.0-rc.1").is_none());
    }

    #[test]
    fn update_check_treats_stable_release_as_newer_than_matching_rc_install() {
        let check = update_check_from_release("0.1.0-rc.1", release("v0.1.0"));

        assert_eq!(check.status, AppUpdateStatus::UpdateAvailable);
        assert!(check.update_available);
        assert_eq!(check.latest_stable_version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn update_check_ignores_prerelease_latest_metadata() {
        let mut release = release("v0.1.1-rc.1");
        release.prerelease = true;

        let check = update_check_from_release("0.1.0", release);

        assert_eq!(check.status, AppUpdateStatus::Unavailable);
        assert!(!check.update_available);
        assert_eq!(check.latest_stable_version, None);
    }

    #[test]
    fn update_check_can_report_no_stable_release_without_error() {
        let check = AppUpdateCheck {
            installed_version: "0.1.0-rc.1".into(),
            latest_stable_version: None,
            latest_stable_tag: None,
            release_url: None,
            update_available: false,
            status: AppUpdateStatus::NoStableRelease,
            message: "No stable release has been published yet.".into(),
        };

        assert_eq!(check.status, AppUpdateStatus::NoStableRelease);
        assert!(!check.update_available);
        assert_eq!(check.latest_stable_version, None);
    }

    #[test]
    fn update_check_reports_up_to_date_for_same_stable_version() {
        let check = update_check_from_release("0.1.0", release("v0.1.0"));

        assert_eq!(check.status, AppUpdateStatus::UpToDate);
        assert!(!check.update_available);
        assert_eq!(check.latest_stable_tag.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn update_check_handles_malformed_latest_release_tags() {
        let check = update_check_from_release("0.1.0", release("developer-preview"));

        assert_eq!(check.status, AppUpdateStatus::Unavailable);
        assert!(!check.update_available);
    }
}
