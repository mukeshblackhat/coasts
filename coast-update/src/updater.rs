/// Self-update logic: download tarball, extract, and atomically replace binaries.
use crate::error::UpdateError;
use semver::Version;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn resolved_binary_names(current_name: &str) -> (String, String) {
    if current_name == "coastd" || current_name.starts_with("coastd-") {
        let suffix = &current_name["coastd".len()..];
        return (format!("coast{suffix}"), format!("coastd{suffix}"));
    }
    if current_name == "coast" || current_name.starts_with("coast-") {
        let suffix = &current_name["coast".len()..];
        return (format!("coast{suffix}"), format!("coastd{suffix}"));
    }
    ("coast".to_string(), "coastd".to_string())
}

/// Find the `coast` and `coastd` binary paths using the current executable name.
///
/// When the updater is invoked from `coastd`, this still resolves the matching
/// `coast` sibling so both binaries can be updated together.
pub fn resolve_binary_paths() -> (PathBuf, PathBuf) {
    if let Ok(exe) = std::env::current_exe() {
        let current_name = exe.file_name().unwrap_or_default().to_string_lossy();
        let (coast_name, coastd_name) = resolved_binary_names(&current_name);
        if let Some(dir) = exe.parent() {
            let coast_path = dir.join(&coast_name);
            let coastd_path = dir.join(&coastd_name);
            return (
                if coast_path.exists() {
                    coast_path
                } else {
                    PathBuf::from(coast_name)
                },
                if coastd_path.exists() {
                    coastd_path
                } else {
                    PathBuf::from(coastd_name)
                },
            );
        }
    }
    (PathBuf::from("coast"), PathBuf::from("coastd"))
}

/// Find the `coastd` binary path, using the same resolution logic as the CLI.
pub fn resolve_coastd_path() -> PathBuf {
    let (_coast_path, coastd_path) = resolve_binary_paths();
    coastd_path
}

/// Detect the current platform and return (os, arch) strings matching
/// the release tarball naming convention.
pub fn current_platform() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "amd64"
    } else {
        "unknown"
    };

    (os, arch)
}

/// Download a release tarball to a temporary file and return its path.
pub async fn download_release(
    version: &Version,
    timeout: Duration,
) -> Result<PathBuf, UpdateError> {
    let (os, arch) = current_platform();
    let url = crate::checker::release_tarball_url(version, os, arch);

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("coast-cli")
        .build()
        .map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(UpdateError::DownloadFailed(format!(
            "HTTP {} from {url}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;

    let tmp_dir = std::env::temp_dir().join("coast-update");
    std::fs::create_dir_all(&tmp_dir)?;
    let tarball_path = tmp_dir.join(format!("coast-v{version}-{os}-{arch}.tar.gz"));
    std::fs::write(&tarball_path, &bytes)?;

    Ok(tarball_path)
}

/// Extract the tarball and atomically replace the `coast` and `coastd` binaries.
///
/// The replacement strategy:
/// 1. Extract to a temp directory
/// 2. For each binary, rename the old one to `.old`, move the new one in place
/// 3. Remove the `.old` files
///
/// This is as close to atomic as we can get on most filesystems.
pub fn apply_update(tarball_path: &Path) -> Result<(), UpdateError> {
    let (coast_path, coastd_path) = resolve_binary_paths();

    let extract_dir = tarball_path
        .parent()
        .unwrap_or(Path::new("/tmp"))
        .join("extracted");
    std::fs::create_dir_all(&extract_dir)?;

    // Extract using tar (available on macOS and Linux)
    let status = std::process::Command::new("tar")
        .args(["xzf", &tarball_path.to_string_lossy(), "-C"])
        .arg(&extract_dir)
        .status()
        .map_err(|e| UpdateError::ApplyFailed(format!("Failed to run tar: {e}")))?;

    if !status.success() {
        return Err(UpdateError::ApplyFailed(
            "tar extraction failed".to_string(),
        ));
    }

    // Find extracted binaries
    let new_coast = extract_dir.join("coast");
    let new_coastd = extract_dir.join("coastd");

    if !new_coast.exists() {
        return Err(UpdateError::ApplyFailed(
            "Tarball does not contain 'coast' binary".to_string(),
        ));
    }
    if !new_coastd.exists() {
        return Err(UpdateError::ApplyFailed(
            "Tarball does not contain 'coastd' binary".to_string(),
        ));
    }

    if coast_path.is_absolute() || coast_path.exists() {
        replace_binary(&new_coast, &coast_path)?;
    }

    if coastd_path.is_absolute() || coastd_path.exists() {
        replace_binary(&new_coastd, &coastd_path)?;
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&extract_dir);
    let _ = std::fs::remove_file(tarball_path);

    Ok(())
}

/// Replace a single binary atomically using rename, falling back to sudo cp
/// when the install directory is not writable.
fn replace_binary(new_path: &Path, target_path: &Path) -> Result<(), UpdateError> {
    let backup = target_path.with_extension("old");

    // Move current binary out of the way
    if target_path.exists() {
        match std::fs::rename(target_path, &backup) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return sudo_copy(new_path, target_path);
            }
            Err(e) => {
                return Err(UpdateError::ApplyFailed(format!(
                    "Failed to backup {}: {e}",
                    target_path.display()
                )));
            }
        }
    }

    // Move new binary into place
    if let Err(e) = std::fs::rename(new_path, target_path) {
        // Try to restore backup
        if backup.exists() {
            let _ = std::fs::rename(&backup, target_path);
        }
        return Err(UpdateError::ApplyFailed(format!(
            "Failed to install new binary at {}: {e}",
            target_path.display()
        )));
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(target_path, std::fs::Permissions::from_mode(0o755));
    }

    // Remove backup
    let _ = std::fs::remove_file(&backup);

    Ok(())
}

/// Copy a binary into place using sudo, prompting the user for their password.
fn sudo_copy(new_path: &Path, target_path: &Path) -> Result<(), UpdateError> {
    eprintln!("sudo access required to update {}", target_path.display());

    let status = std::process::Command::new("sudo")
        .args(["cp", "-f"])
        .arg(new_path)
        .arg(target_path)
        .status()
        .map_err(|e| UpdateError::ApplyFailed(format!("Failed to run sudo: {e}")))?;

    if !status.success() {
        return Err(UpdateError::ApplyFailed(format!(
            "sudo cp failed for {}",
            target_path.display()
        )));
    }

    // Ensure correct permissions
    let chmod_status = std::process::Command::new("sudo")
        .args(["chmod", "755"])
        .arg(target_path)
        .status();

    if let Ok(s) = chmod_status {
        if !s.success() {
            eprintln!(
                "warning: failed to set permissions on {}",
                target_path.display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform_valid() {
        let (os, arch) = current_platform();
        assert!(
            os == "darwin" || os == "linux" || os == "unknown",
            "unexpected os: {os}"
        );
        assert!(
            arch == "arm64" || arch == "amd64" || arch == "unknown",
            "unexpected arch: {arch}"
        );
    }

    #[test]
    fn test_resolve_coastd_path_returns_something() {
        let path = resolve_coastd_path();
        // Should return either an absolute path or "coastd"
        assert!(
            path.is_absolute() || path == PathBuf::from("coastd"),
            "unexpected coastd path: {}",
            path.display()
        );
    }

    #[test]
    fn test_resolved_binary_names_from_cli_binary() {
        assert_eq!(
            resolved_binary_names("coast"),
            ("coast".to_string(), "coastd".to_string())
        );
        assert_eq!(
            resolved_binary_names("coast-dev"),
            ("coast-dev".to_string(), "coastd-dev".to_string())
        );
    }

    #[test]
    fn test_resolved_binary_names_from_daemon_binary() {
        assert_eq!(
            resolved_binary_names("coastd"),
            ("coast".to_string(), "coastd".to_string())
        );
        assert_eq!(
            resolved_binary_names("coastd-dev"),
            ("coast-dev".to_string(), "coastd-dev".to_string())
        );
    }

    #[test]
    fn test_replace_binary_with_temp_files() {
        let dir = tempfile::tempdir().unwrap();

        let old_binary = dir.path().join("coast");
        std::fs::write(&old_binary, b"old-binary").unwrap();

        let new_binary = dir.path().join("coast-new");
        std::fs::write(&new_binary, b"new-binary").unwrap();

        replace_binary(&new_binary, &old_binary).unwrap();

        let content = std::fs::read_to_string(&old_binary).unwrap();
        assert_eq!(content, "new-binary");
        assert!(!new_binary.exists(), "source file should be renamed away");
    }

    #[test]
    fn test_replace_binary_no_existing_target() {
        let dir = tempfile::tempdir().unwrap();

        let target = dir.path().join("coast");
        let new_binary = dir.path().join("coast-new");
        std::fs::write(&new_binary, b"fresh-binary").unwrap();

        replace_binary(&new_binary, &target).unwrap();

        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content, "fresh-binary");
    }

    #[test]
    fn test_replace_binary_restore_on_failure() {
        let dir = tempfile::tempdir().unwrap();

        let target = dir.path().join("coast");
        std::fs::write(&target, b"original").unwrap();

        // new_binary doesn't exist — rename will fail
        let new_binary = dir.path().join("nonexistent");

        let result = replace_binary(&new_binary, &target);
        assert!(result.is_err());

        // Original should be restored from backup
        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content, "original");
    }

    #[test]
    fn test_apply_update_missing_tarball() {
        let result = apply_update(Path::new("/nonexistent/coast.tar.gz"));
        assert!(result.is_err());
    }
}
