use tracing::info;

use coast_core::protocol::BuildProgressEvent;
use coast_docker::runtime::Runtime;

use super::emit;

/// Filter tarballs to only those whose images aren't already present in the inner daemon.
///
/// Returns `(tarballs_to_load, skipped_count)`.
pub(super) fn filter_tarballs_to_load(
    tarball_names: Vec<String>,
    existing_images: &std::collections::HashSet<String>,
) -> (Vec<String>, usize) {
    let total = tarball_names.len();
    let to_load = if existing_images.is_empty() {
        tarball_names
    } else {
        tarball_names
            .into_iter()
            .filter(|tarball_name| {
                !existing_images.iter().any(|img| {
                    let safe = img.replace(['/', ':'], "_");
                    tarball_name.starts_with(&safe)
                })
            })
            .collect()
    };
    let skipped = total - to_load.len();
    (to_load, skipped)
}

/// Collect tarball filenames from the image cache directory, filtering to
/// this project's coast-built images plus all base images.
pub(super) fn collect_project_tarballs(cache_dir: &std::path::Path, project: &str) -> Vec<String> {
    let project_prefix = format!("coast-built_{}_", project.replace(['/', ':'], "_"));

    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "tar"))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            if name.starts_with(&project_prefix) {
                return true;
            }
            if name.starts_with("coast-built_") {
                return false;
            }
            true
        })
        .collect()
}

/// Query the inner daemon for images that are already loaded.
pub(super) async fn query_existing_images(
    docker: &bollard::Docker,
    container_id: &str,
) -> std::collections::HashSet<String> {
    let check_rt = coast_docker::dind::DindRuntime::with_client(docker.clone());
    match check_rt
        .exec_in_coast(
            container_id,
            &["docker", "images", "--format", "{{.Repository}}:{{.Tag}}"],
        )
        .await
    {
        Ok(result) if result.success() => result
            .stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && l != "<none>:<none>")
            .collect(),
        _ => std::collections::HashSet::new(),
    }
}

/// Load filtered tarballs into the inner daemon with parallel execution (max 4 concurrent).
pub(super) async fn load_tarballs_into_inner_daemon(
    tarballs: &[String],
    docker: &bollard::Docker,
    container_id: &str,
    progress: &tokio::sync::mpsc::Sender<BuildProgressEvent>,
) {
    if tarballs.is_empty() {
        return;
    }

    let load_count = tarballs.len();
    let load_commands = crate::image_loader::load_all_images_commands(
        tarballs,
        crate::image_loader::IMAGE_CACHE_CONTAINER_PATH,
    );

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut load_handles = Vec::new();
    for cmd in load_commands {
        let sem = semaphore.clone();
        let load_rt = coast_docker::dind::DindRuntime::with_client(docker.clone());
        let cid = container_id.to_string();
        load_handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let cmd_refs: Vec<&str> = cmd.iter().map(std::string::String::as_str).collect();
            if let Err(e) = load_rt.exec_in_coast(&cid, &cmd_refs).await {
                tracing::warn!(error = %e, cmd = ?cmd, "failed to load cached image, continuing");
            }
        }));
    }
    for handle in load_handles {
        let _ = handle.await;
    }
    emit(
        progress,
        BuildProgressEvent::item(
            "Loading cached images",
            format!("{} loaded", load_count),
            "ok",
        ),
    );
}

/// Pipe per-instance images from the host daemon into the inner daemon via `docker save | docker load`.
pub(super) async fn pipe_host_images_to_inner_daemon(
    per_instance_image_tags: &[(String, String)],
    container_id: &str,
) {
    if per_instance_image_tags.is_empty() {
        return;
    }

    let mut pipe_handles = Vec::new();
    for (service_name, tag) in per_instance_image_tags {
        pipe_handles.push(spawn_host_image_pipe_task(service_name, tag, container_id));
    }

    for handle in pipe_handles {
        handle_host_image_pipe_result(handle.await);
    }
}

struct HostImagePipeResult {
    service_name: String,
    tag: String,
    load_output: std::process::Output,
}

fn spawn_host_image_pipe_task(
    service_name: &str,
    tag: &str,
    container_id: &str,
) -> tokio::task::JoinHandle<Result<HostImagePipeResult, std::io::Error>> {
    info!(
        service = %service_name,
        tag = %tag,
        "loading per-instance image into inner daemon"
    );
    let tag_owned = tag.to_string();
    let container_id_owned = container_id.to_string();
    let service_name_owned = service_name.to_string();
    tokio::task::spawn_blocking(move || {
        pipe_host_image_into_inner_daemon(&service_name_owned, &tag_owned, &container_id_owned)
    })
}

fn pipe_host_image_into_inner_daemon(
    service_name: &str,
    tag: &str,
    container_id: &str,
) -> Result<HostImagePipeResult, std::io::Error> {
    let mut save = std::process::Command::new("docker")
        .args(["save", tag])
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let save_stdout = save.stdout.take().expect("save stdout was piped");
    let load_output = std::process::Command::new("docker")
        .args(["exec", "-i", container_id, "docker", "load"])
        .stdin(save_stdout)
        .output()?;
    save.wait()?;

    Ok(HostImagePipeResult {
        service_name: service_name.to_string(),
        tag: tag.to_string(),
        load_output,
    })
}

fn handle_host_image_pipe_result(
    result: Result<Result<HostImagePipeResult, std::io::Error>, tokio::task::JoinError>,
) {
    match result {
        Ok(pipe_result) => handle_host_image_pipe_outcome(pipe_result),
        Err(error) => {
            tracing::warn!(error = %error, "spawn_blocking failed for image piping");
        }
    }
}

fn handle_host_image_pipe_outcome(result: Result<HostImagePipeResult, std::io::Error>) {
    match result {
        Ok(pipe_result) => log_host_image_load_output(&pipe_result),
        Err(error) => tracing::warn!(error = %error, "failed to pipe image into inner daemon"),
    }
}

fn log_host_image_load_output(pipe_result: &HostImagePipeResult) {
    if pipe_result.load_output.status.success() {
        info!(
            service = %pipe_result.service_name,
            tag = %pipe_result.tag,
            "per-instance image loaded into inner daemon"
        );
        return;
    }

    let stderr = String::from_utf8_lossy(&pipe_result.load_output.stderr);
    tracing::warn!(
        service = %pipe_result.service_name,
        tag = %pipe_result.tag,
        stderr = %stderr,
        "failed to load per-instance image into inner daemon"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    // --- filter_tarballs_to_load ---

    #[test]
    fn test_filter_tarballs_all_new() {
        let tarballs = vec![
            "node_20-alpine_abc123.tar".to_string(),
            "postgres_16_def456.tar".to_string(),
        ];
        let existing = std::collections::HashSet::new();
        let (to_load, skipped) = filter_tarballs_to_load(tarballs, &existing);
        assert_eq!(to_load.len(), 2);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_filter_tarballs_some_existing() {
        let tarballs = vec![
            "node_20-alpine_abc123.tar".to_string(),
            "postgres_16_def456.tar".to_string(),
        ];
        let existing: std::collections::HashSet<String> =
            ["node:20-alpine".to_string()].into_iter().collect();
        let (to_load, skipped) = filter_tarballs_to_load(tarballs, &existing);
        assert_eq!(to_load.len(), 1);
        assert_eq!(to_load[0], "postgres_16_def456.tar");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_tarballs_all_existing() {
        let tarballs = vec!["node_20-alpine_abc123.tar".to_string()];
        let existing: std::collections::HashSet<String> =
            ["node:20-alpine".to_string()].into_iter().collect();
        let (to_load, skipped) = filter_tarballs_to_load(tarballs, &existing);
        assert!(to_load.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_tarballs_image_with_slash_in_name() {
        let tarballs = vec!["library_nginx_1.25_abc.tar".to_string()];
        let existing: std::collections::HashSet<String> =
            ["library/nginx:1.25".to_string()].into_iter().collect();
        let (to_load, skipped) = filter_tarballs_to_load(tarballs, &existing);
        assert!(to_load.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_filter_tarballs_empty_input() {
        let (to_load, skipped) = filter_tarballs_to_load(vec![], &std::collections::HashSet::new());
        assert!(to_load.is_empty());
        assert_eq!(skipped, 0);
    }

    // --- collect_project_tarballs ---

    #[test]
    fn test_collect_project_tarballs_nonexistent_dir() {
        let result = collect_project_tarballs(std::path::Path::new("/nonexistent"), "my-proj");
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_project_tarballs_includes_own_and_base_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("node_20-alpine_abc.tar"), "").unwrap();
        std::fs::write(dir.path().join("coast-built_my-proj_web_def.tar"), "").unwrap();
        std::fs::write(dir.path().join("coast-built_other-proj_api_ghi.tar"), "").unwrap();
        // Non-.tar file should be excluded
        std::fs::write(dir.path().join("readme.txt"), "").unwrap();

        let result = collect_project_tarballs(dir.path(), "my-proj");
        assert!(result.contains(&"node_20-alpine_abc.tar".to_string()));
        assert!(result.contains(&"coast-built_my-proj_web_def.tar".to_string()));
        assert!(!result.contains(&"coast-built_other-proj_api_ghi.tar".to_string()));
        assert!(!result.contains(&"readme.txt".to_string()));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_collect_project_tarballs_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let result = collect_project_tarballs(dir.path(), "my-proj");
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_project_tarballs_project_with_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("coast-built_my_org_app_svc_abc.tar"), "").unwrap();
        std::fs::write(dir.path().join("coast-built_other_proj_svc_abc.tar"), "").unwrap();

        let result = collect_project_tarballs(dir.path(), "my/org:app");
        assert!(result.contains(&"coast-built_my_org_app_svc_abc.tar".to_string()));
        assert!(!result.contains(&"coast-built_other_proj_svc_abc.tar".to_string()));
    }

    #[test]
    fn test_log_host_image_load_output_handles_success_status() {
        let result = HostImagePipeResult {
            service_name: "web".to_string(),
            tag: "coast-built:web".to_string(),
            load_output: std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        };

        log_host_image_load_output(&result);
    }

    #[test]
    fn test_handle_host_image_pipe_outcome_handles_io_error() {
        let error = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "boom");
        handle_host_image_pipe_outcome(Err(error));
    }
}
