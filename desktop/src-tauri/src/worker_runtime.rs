// SPDX-License-Identifier: AGPL-3.0-only
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::errors::{HostError, HostResult};

const BUNDLED_RUNTIME_ROOT: &str = "runtime";
const BUNDLED_RUNTIME_MANIFEST: &str = "runtime-manifest.json";
const BUNDLED_RUNTIME_MANIFEST_VERSION: u32 = 1;
const BUNDLED_PADDLE_CACHE_DIR: &str = "paddle-models";

#[derive(Debug)]
pub(crate) struct WorkerLaunchSpec {
    pub(crate) current_dir: PathBuf,
    pub(crate) python_executable: PathBuf,
    pub(crate) python_path: Option<OsString>,
    pub(crate) remove_env: Vec<OsString>,
    pub(crate) extra_env: Vec<(OsString, OsString)>,
}

#[derive(Clone, Copy)]
pub(crate) enum WorkerRuntimeSource {
    RepoFallback,
    BundledRequired,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundledRuntimeManifest {
    manifest_version: u32,
    python_executable: String,
    #[serde(default)]
    python_path_entries: Vec<String>,
}

pub(crate) fn resolve_worker_launch_spec(
    bundled_resource_dir: Option<&Path>,
    source: WorkerRuntimeSource,
) -> HostResult<WorkerLaunchSpec> {
    match (bundled_resource_dir, source) {
        (Some(resource_dir), WorkerRuntimeSource::BundledRequired) => {
            resolve_bundled_runtime_launch_spec(resource_dir)
        }
        (Some(_resource_dir), WorkerRuntimeSource::RepoFallback) => resolve_repo_launch_spec(),
        (None, _) => resolve_repo_launch_spec(),
    }
}

fn resolve_bundled_runtime_launch_spec(resource_dir: &Path) -> HostResult<WorkerLaunchSpec> {
    let runtime_root = resource_dir.join(BUNDLED_RUNTIME_ROOT);
    let manifest_path = runtime_root.join(BUNDLED_RUNTIME_MANIFEST);
    if !manifest_path.is_file() {
        return Err(HostError::Worker(format!(
            "Bundled desktop runtime manifest was not found at '{}'.",
            manifest_path.display()
        )));
    }
    bundled_runtime_launch_spec_from_manifest(resource_dir, &runtime_root, &manifest_path)
}

fn bundled_runtime_launch_spec_from_manifest(
    resource_dir: &Path,
    runtime_root: &Path,
    manifest_path: &Path,
) -> HostResult<WorkerLaunchSpec> {
    let manifest = load_bundled_runtime_manifest(manifest_path)?;
    let python_executable = resolve_manifest_path(runtime_root, &manifest.python_executable);
    let python_path = resolve_python_path(runtime_root, &manifest.python_path_entries)?;

    Ok(WorkerLaunchSpec {
        current_dir: runtime_root.to_path_buf(),
        python_executable,
        python_path,
        remove_env: bundled_runtime_removed_env(),
        extra_env: bundled_paddle_model_env(resource_dir)?,
    })
}

fn load_bundled_runtime_manifest(manifest_path: &Path) -> HostResult<BundledRuntimeManifest> {
    let manifest_json = std::fs::read_to_string(manifest_path).map_err(|err| {
        HostError::Worker(format!(
            "Could not read bundled desktop runtime manifest '{}': {err}",
            manifest_path.display()
        ))
    })?;
    let manifest: BundledRuntimeManifest = serde_json::from_str(&manifest_json).map_err(|err| {
        HostError::Worker(format!(
            "Bundled desktop runtime manifest '{}' is invalid JSON: {err}",
            manifest_path.display()
        ))
    })?;
    if manifest.manifest_version != BUNDLED_RUNTIME_MANIFEST_VERSION {
        return Err(HostError::Worker(format!(
            "Bundled desktop runtime manifest '{}' uses version {} but the host expects {}.",
            manifest_path.display(),
            manifest.manifest_version,
            BUNDLED_RUNTIME_MANIFEST_VERSION
        )));
    }
    Ok(manifest)
}

fn resolve_repo_launch_spec() -> HostResult<WorkerLaunchSpec> {
    let repo_root = repo_root()?;
    let python_executable = resolve_repo_python(&repo_root)?;
    let python_path = resolve_python_path(&repo_root, &[String::from("cli/src")])?;

    Ok(WorkerLaunchSpec {
        current_dir: repo_root,
        python_executable,
        python_path,
        remove_env: Vec::new(),
        extra_env: paddle_model_env(dev_checkout_paddle_model_dir()?),
    })
}

fn bundled_runtime_removed_env() -> Vec<OsString> {
    vec![OsString::from("PYTHONHOME")]
}

fn resolve_repo_python(repo_root: &Path) -> HostResult<PathBuf> {
    if let Ok(explicit) = std::env::var("SCRIPTSCORE_PYTHON") {
        return Ok(PathBuf::from(explicit));
    }
    let cli_root = repo_root.join("cli");
    let unix_candidate = cli_root.join(".venv").join("bin").join("python");
    if unix_candidate.is_file() {
        return Ok(unix_candidate);
    }
    let windows_candidate = cli_root.join(".venv").join("Scripts").join("python.exe");
    if windows_candidate.is_file() {
        return Ok(windows_candidate);
    }
    Ok(PathBuf::from(if cfg!(windows) {
        "python"
    } else {
        "python3"
    }))
}

fn resolve_python_path(runtime_root: &Path, entries: &[String]) -> HostResult<Option<OsString>> {
    if entries.is_empty() {
        return Ok(None);
    }
    let resolved_entries = entries
        .iter()
        .map(|entry| resolve_manifest_path(runtime_root, entry))
        .collect::<Vec<_>>();
    let joined = std::env::join_paths(resolved_entries).map_err(|err| {
        HostError::Worker(format!(
            "Could not prepare PYTHONPATH for desktop worker launch: {err}"
        ))
    })?;
    Ok(Some(joined))
}

fn resolve_manifest_path(runtime_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        runtime_root.join(path)
    }
}

fn repo_root() -> HostResult<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| HostError::Worker("Could not resolve the repository root.".into()))
}

fn dev_checkout_paddle_model_dir() -> HostResult<PathBuf> {
    Ok(repo_root()?.join("cli/models/paddle"))
}

fn bundled_paddle_model_env(resource_dir: &Path) -> HostResult<Vec<(OsString, OsString)>> {
    let packaged_model_dir = resource_dir.join("models/paddle");
    if !is_valid_paddle_model_dir(&packaged_model_dir) {
        return Ok(Vec::new());
    }
    let writable_model_dir = prepare_writable_paddle_model_dir(&packaged_model_dir)?;
    Ok(paddle_model_env(writable_model_dir))
}

fn paddle_model_env(candidate: PathBuf) -> Vec<(OsString, OsString)> {
    if is_valid_paddle_model_dir(&candidate) {
        let candidate = candidate.into_os_string();
        vec![
            (
                OsString::from("SCRIPTSCORE_DETECT_PADDLE_MODEL_DIR"),
                candidate.clone(),
            ),
            (
                OsString::from("SCRIPTSCORE_OCR_PADDLE_MODEL_DIR"),
                candidate.clone(),
            ),
            (
                OsString::from("SCRIPTSCORE_PII_PADDLE_MODEL_DIR"),
                candidate,
            ),
        ]
    } else {
        Vec::new()
    }
}

fn is_valid_paddle_model_dir(path: &Path) -> bool {
    ["det", "rec"].iter().all(|name| {
        let model_dir = path.join(name);
        model_dir.is_dir()
            && (model_dir.join("inference.json").is_file()
                || model_dir.join("inference.pdmodel").is_file())
            && (!model_dir.join("inference.pdmodel").is_file()
                || model_dir.join("inference.pdiparams").is_file())
    })
}

fn prepare_writable_paddle_model_dir(packaged_model_dir: &Path) -> HostResult<PathBuf> {
    let fingerprint = paddle_model_fingerprint(packaged_model_dir)?;
    let cache_root = bundled_paddle_cache_root()?;
    let target = cache_root.join(fingerprint);
    if is_valid_paddle_model_dir(&target) {
        return Ok(target);
    }

    fs::create_dir_all(&cache_root).map_err(|err| {
        HostError::Worker(format!(
            "Could not create bundled Paddle model cache '{}': {err}",
            cache_root.display()
        ))
    })?;
    let staging = cache_root.join(format!(
        ".{}-{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("models"),
        std::process::id()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|err| {
            HostError::Worker(format!(
                "Could not reset bundled Paddle model staging directory '{}': {err}",
                staging.display()
            ))
        })?;
    }
    copy_dir(packaged_model_dir, &staging)?;
    if target.exists() {
        if is_valid_paddle_model_dir(&target) {
            let _ = fs::remove_dir_all(&staging);
            return Ok(target);
        }
        fs::remove_dir_all(&target).map_err(|err| {
            HostError::Worker(format!(
                "Could not replace bundled Paddle model cache '{}': {err}",
                target.display()
            ))
        })?;
    }
    fs::rename(&staging, &target).map_err(|err| {
        let _ = fs::remove_dir_all(&staging);
        HostError::Worker(format!(
            "Could not activate bundled Paddle model cache '{}': {err}",
            target.display()
        ))
    })?;
    Ok(target)
}

fn bundled_paddle_cache_root() -> HostResult<PathBuf> {
    dirs::cache_dir()
        .map(|cache_dir| {
            cache_dir
                .join("scriptscore-desktop")
                .join(BUNDLED_PADDLE_CACHE_DIR)
        })
        .ok_or_else(|| {
            HostError::Worker(
                "Could not resolve a writable cache directory for Paddle models.".into(),
            )
        })
}

fn paddle_model_fingerprint(root: &Path) -> HostResult<String> {
    let mut hasher = Sha256::new();
    for file_path in model_files(root)? {
        let relative = file_path.strip_prefix(root).map_err(|err| {
            HostError::Worker(format!(
                "Could not fingerprint bundled Paddle model file '{}': {err}",
                file_path.display()
            ))
        })?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);

        let mut file = fs::File::open(&file_path).map_err(|err| {
            HostError::Worker(format!(
                "Could not read bundled Paddle model file '{}': {err}",
                file_path.display()
            ))
        })?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|err| {
                HostError::Worker(format!(
                    "Could not hash bundled Paddle model file '{}': {err}",
                    file_path.display()
                ))
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hasher.update([0]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn model_files(root: &Path) -> HostResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_model_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_model_files(dir: &Path, files: &mut Vec<PathBuf>) -> HostResult<()> {
    for entry in fs::read_dir(dir).map_err(|err| {
        HostError::Worker(format!(
            "Could not list bundled Paddle model directory '{}': {err}",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            HostError::Worker(format!(
                "Could not inspect bundled Paddle model directory '{}': {err}",
                dir.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|err| {
            HostError::Worker(format!(
                "Could not inspect bundled Paddle model path '{}': {err}",
                path.display()
            ))
        })?;
        if file_type.is_dir() {
            collect_model_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn copy_dir(source: &Path, target: &Path) -> HostResult<()> {
    fs::create_dir_all(target).map_err(|err| {
        HostError::Worker(format!(
            "Could not create bundled Paddle model cache directory '{}': {err}",
            target.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|err| {
        HostError::Worker(format!(
            "Could not list bundled Paddle model directory '{}': {err}",
            source.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            HostError::Worker(format!(
                "Could not inspect bundled Paddle model directory '{}': {err}",
                source.display()
            ))
        })?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| {
            HostError::Worker(format!(
                "Could not inspect bundled Paddle model path '{}': {err}",
                source_path.display()
            ))
        })?;
        if file_type.is_dir() {
            copy_dir(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).map_err(|err| {
                HostError::Worker(format!(
                    "Could not copy bundled Paddle model file '{}' to '{}': {err}",
                    source_path.display(),
                    target_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::{
        is_valid_paddle_model_dir, resolve_worker_launch_spec, WorkerLaunchSpec,
        WorkerRuntimeSource,
    };

    fn temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    #[test]
    fn bundled_runtime_manifest_supports_relative_python_and_pythonpath_entries() {
        let resource_dir = temp_dir("scriptscore-worker-runtime");
        let runtime_root = resource_dir.join("runtime");
        fs::create_dir_all(runtime_root.join("python/bin")).expect("runtime dir should create");
        fs::create_dir_all(runtime_root.join("cli-src")).expect("cli source dir should create");
        write_paddle_model_layout(&resource_dir.join("models/paddle"));
        fs::write(
            runtime_root.join("runtime-manifest.json"),
            r#"{
  "manifestVersion": 1,
  "pythonExecutable": "python/bin/python3",
  "pythonPathEntries": ["cli-src"]
}"#,
        )
        .expect("manifest should write");

        let spec =
            resolve_worker_launch_spec(Some(&resource_dir), WorkerRuntimeSource::BundledRequired)
                .expect("bundled runtime spec should resolve");

        assert_eq!(spec.current_dir, runtime_root);
        assert_eq!(
            spec.python_executable,
            runtime_root.join("python/bin/python3")
        );
        assert_eq!(
            spec.python_path.as_ref().expect("python path should exist"),
            &runtime_root.join("cli-src").into_os_string()
        );
        assert_eq!(spec.remove_env, vec![OsString::from("PYTHONHOME")]);
        let cached_model_dir = env_model_dir(&spec, "SCRIPTSCORE_DETECT_PADDLE_MODEL_DIR");
        assert_ne!(cached_model_dir, resource_dir.join("models/paddle"));
        assert!(is_valid_paddle_model_dir(&cached_model_dir));
        fs::write(cached_model_dir.join(".write-test"), "ok")
            .expect("cached models should be writable");
        let _ = fs::remove_file(cached_model_dir.join(".write-test"));
        assert_eq!(
            spec.extra_env,
            vec![
                (
                    "SCRIPTSCORE_DETECT_PADDLE_MODEL_DIR".into(),
                    cached_model_dir.clone().into_os_string(),
                ),
                (
                    "SCRIPTSCORE_OCR_PADDLE_MODEL_DIR".into(),
                    cached_model_dir.clone().into_os_string(),
                ),
                (
                    "SCRIPTSCORE_PII_PADDLE_MODEL_DIR".into(),
                    cached_model_dir.clone().into_os_string(),
                ),
            ]
        );

        let _ = fs::remove_dir_all(resource_dir);
        let _ = fs::remove_dir_all(cached_model_dir);
    }

    #[test]
    fn bundled_runtime_manifest_supports_absolute_python_paths() {
        let resource_dir = temp_dir("scriptscore-worker-runtime-absolute");
        let runtime_root = resource_dir.join("runtime");
        let absolute_python = std::env::temp_dir().join("scriptscore-python");
        fs::create_dir_all(&runtime_root).expect("runtime dir should create");
        fs::write(
            runtime_root.join("runtime-manifest.json"),
            json!({
                "manifestVersion": 1,
                "pythonExecutable": absolute_python.to_string_lossy().into_owned(),
                "pythonPathEntries": [],
            })
            .to_string(),
        )
        .expect("manifest should write");

        let spec =
            resolve_worker_launch_spec(Some(&resource_dir), WorkerRuntimeSource::BundledRequired)
                .expect("bundled runtime spec should resolve");

        assert_eq!(spec.python_executable, absolute_python);
        assert!(spec.python_path.is_none());
        assert_eq!(spec.remove_env, vec![OsString::from("PYTHONHOME")]);
        assert!(spec.extra_env.is_empty());

        let _ = fs::remove_dir_all(resource_dir);
    }

    #[test]
    fn repo_fallback_prefers_live_repo_source_even_when_runtime_bundle_exists() {
        let resource_dir = temp_dir("scriptscore-worker-runtime-fallback");
        let runtime_root = resource_dir.join("runtime");
        fs::create_dir_all(runtime_root.join("python/bin")).expect("runtime dir should create");
        fs::create_dir_all(runtime_root.join("cli-src")).expect("cli source dir should create");
        fs::write(
            runtime_root.join("runtime-manifest.json"),
            r#"{
  "manifestVersion": 1,
  "pythonExecutable": "python/bin/python3",
  "pythonPathEntries": ["cli-src"]
}"#,
        )
        .expect("manifest should write");

        let spec =
            resolve_worker_launch_spec(Some(&resource_dir), WorkerRuntimeSource::RepoFallback)
                .expect("repo fallback runtime spec should resolve");

        assert_ne!(spec.current_dir, runtime_root);
        assert!(spec.current_dir.join("cli/src/scriptscore").is_dir());
        assert_eq!(
            spec.python_path.expect("repo python path should exist"),
            spec.current_dir.join("cli/src").into_os_string(),
        );
        assert!(spec.remove_env.is_empty());

        let _ = fs::remove_dir_all(resource_dir);
    }

    #[test]
    fn bundled_required_errors_when_manifest_is_missing() {
        let resource_dir = temp_dir("scriptscore-worker-runtime-missing");
        fs::create_dir_all(resource_dir.join("runtime")).expect("runtime dir should create");

        let error =
            resolve_worker_launch_spec(Some(&resource_dir), WorkerRuntimeSource::BundledRequired)
                .expect_err("missing bundled manifest should fail");

        assert!(error
            .to_string()
            .contains("Bundled desktop runtime manifest was not found"));

        let _ = fs::remove_dir_all(resource_dir);
    }

    fn write_paddle_model_layout(root: &Path) {
        for name in ["det", "rec"] {
            let model_dir = root.join(name);
            fs::create_dir_all(&model_dir).expect("model dir should create");
            fs::write(model_dir.join("inference.pdmodel"), "model").expect("model should write");
            fs::write(model_dir.join("inference.pdiparams"), "params")
                .expect("params should write");
        }
    }

    fn env_model_dir(spec: &WorkerLaunchSpec, env_name: &str) -> PathBuf {
        spec.extra_env
            .iter()
            .find_map(|(key, value)| {
                if key == env_name {
                    Some(PathBuf::from(value.clone()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("{env_name} should be set"))
    }
}
