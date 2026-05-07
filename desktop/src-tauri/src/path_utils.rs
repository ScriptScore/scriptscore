// SPDX-License-Identifier: AGPL-3.0-only
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn relative_existing_path(root: &Path, candidate: &Path) -> Option<PathBuf> {
    normalized_relative_existing_path(root, candidate)
        .or_else(|| strip_prefix_platform(candidate, root))
}

pub(crate) fn is_under_existing_dir(root: &Path, candidate: &Path) -> bool {
    relative_existing_path(root, candidate).is_some()
}

fn normalized_relative_existing_path(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let root = normalize_existing_path(root).ok()?;
    let candidate = normalize_existing_path(candidate).ok()?;
    strip_prefix_platform(&candidate, &root)
}

fn normalize_existing_path(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;

    #[cfg(windows)]
    {
        windows_long_path(&canonical).or(Ok(canonical))
    }

    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

#[cfg(not(windows))]
fn strip_prefix_platform(candidate: &Path, root: &Path) -> Option<PathBuf> {
    candidate.strip_prefix(root).ok().map(Path::to_path_buf)
}

#[cfg(windows)]
fn strip_prefix_platform(candidate: &Path, root: &Path) -> Option<PathBuf> {
    let root_components: Vec<_> = root.components().collect();
    let candidate_components: Vec<_> = candidate.components().collect();
    if candidate_components.len() < root_components.len() {
        return None;
    }

    for (candidate_component, root_component) in
        candidate_components.iter().zip(root_components.iter())
    {
        let candidate_text = candidate_component.as_os_str().to_string_lossy();
        let root_text = root_component.as_os_str().to_string_lossy();
        if !candidate_text.eq_ignore_ascii_case(&root_text) {
            return None;
        }
    }

    let mut relative = PathBuf::new();
    for component in &candidate_components[root_components.len()..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

#[cfg(windows)]
fn windows_long_path(path: &Path) -> std::io::Result<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetLongPathNameW;

    let path = strip_verbatim_prefix(path);
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let required_len = unsafe { GetLongPathNameW(wide_path.as_ptr(), std::ptr::null_mut(), 0) };
    if required_len == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut buffer = vec![0; required_len as usize + 1];
    let written_len = unsafe {
        GetLongPathNameW(
            wide_path.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len().try_into().unwrap_or(u32::MAX),
        )
    };
    if written_len == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if written_len as usize >= buffer.len() {
        buffer.resize(written_len as usize + 1, 0);
        let written_len = unsafe {
            GetLongPathNameW(
                wide_path.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len().try_into().unwrap_or(u32::MAX),
            )
        };
        if written_len == 0 || written_len as usize >= buffer.len() {
            return Err(std::io::Error::last_os_error());
        }
        buffer.truncate(written_len as usize);
    } else {
        buffer.truncate(written_len as usize);
    }

    Ok(PathBuf::from(OsString::from_wide(&buffer)))
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let path_text = path.to_string_lossy();
    if let Some(rest) = path_text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = path_text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::relative_existing_path;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_millis()
        ))
    }

    #[test]
    fn relative_existing_path_returns_child_path() {
        let root = temp_root("scriptscore-path-utils");
        let candidate = root.join("artifacts").join("page.png");
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        fs::write(&candidate, b"png").unwrap();

        let relative = relative_existing_path(&root, &candidate).unwrap();

        assert_eq!(relative, PathBuf::from("artifacts").join("page.png"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn relative_existing_path_accepts_windows_case_variants() {
        let root = temp_root("scriptscore-path-utils-case");
        let candidate = root.join("Artifacts").join("Page.png");
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        fs::write(&candidate, b"png").unwrap();
        let root_upper = PathBuf::from(root.to_string_lossy().to_uppercase());

        let relative = super::relative_existing_path(&root_upper, &candidate).unwrap();

        assert_eq!(relative, PathBuf::from("Artifacts").join("Page.png"));
        fs::remove_dir_all(root).unwrap();
    }
}
