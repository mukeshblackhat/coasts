use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecOptions};
use futures_util::StreamExt;
use tracing::{debug, info};

use coast_docker::runtime::{ContainerConfig, ExecResult};

use crate::state::RemoteState;

/// Create and start a DinD container on the remote Docker daemon.
///
/// This mirrors what DindRuntime does, but runs on the remote machine
/// where the Docker daemon lives.
pub async fn create_and_start(
    state: &RemoteState,
    config: &ContainerConfig,
) -> anyhow::Result<String> {
    let docker = &state.docker;
    let container_name = config.container_name();
    let workspace = state.project_mount_path(&config.project);

    info!(container = %container_name, "creating container on remote");

    // Build the host config
    let mut binds = Vec::new();
    let mut tmpfs: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Mount the SSHFS-mounted project as /host-project.
    // The project dir is available via SSHFS at mount_dir/project.
    // Always attempt the mount if the project has a tracked SSHFS mount —
    // exists() can return false for FUSE mounts in some environments.
    let is_mounted = state.mounts.lock().await.contains_key(&config.project);
    if is_mounted || workspace.exists() {
        binds.push(format!(
            "{}:/host-project",
            workspace.to_string_lossy()
        ));
        info!(path = %workspace.display(), "mounting project dir as /host-project");
    } else {
        info!(path = %workspace.display(), "no SSHFS mount found — container will have no project files");
    }

    // Add volume mounts
    for vm in &config.volume_mounts {
        let ro = if vm.read_only { ":ro" } else { "" };
        binds.push(format!("{}:{}{}", vm.volume_name, vm.container_path, ro));
    }

    // Rewrite bind mounts: paths from the local machine get remapped to the SSHFS mount.
    // Paths that already exist on this remote machine are passed through unchanged.
    for bm in &config.bind_mounts {
        let path_str = bm.host_path.to_string_lossy();

        if bm.host_path.exists() {
            // Path exists on this machine (could be SSHFS mount or local path) — use directly
            let ro = if bm.read_only { ":ro" } else { "" };
            let prop = bm.propagation.as_deref().unwrap_or("");
            let prop_suffix = if prop.is_empty() {
                String::new()
            } else {
                format!(":{prop}")
            };
            binds.push(format!(
                "{}:{}{}{}",
                path_str, bm.container_path, ro, prop_suffix
            ));
        } else {
            debug!(path = %path_str, "skipping bind mount (path not available on remote)");
        }
    }

    // Tmpfs mounts
    for t in &config.tmpfs_mounts {
        tmpfs.insert(t.clone(), String::new());
    }

    // Port bindings
    let mut port_bindings = std::collections::HashMap::new();
    let mut exposed_ports = std::collections::HashMap::new();
    for pp in &config.published_ports {
        let container_port = format!("{}/tcp", pp.container_port);
        exposed_ports.insert(container_port.clone(), std::collections::HashMap::new());
        port_bindings.insert(
            container_port,
            Some(vec![bollard::models::PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(pp.host_port.to_string()),
            }]),
        );
    }

    // Environment variables
    let env: Vec<String> = config
        .env_vars
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    // Labels
    let labels: std::collections::HashMap<String, String> = config.labels.clone();

    let host_config = bollard::models::HostConfig {
        privileged: Some(true),
        binds: Some(binds),
        tmpfs: if tmpfs.is_empty() { None } else { Some(tmpfs) },
        port_bindings: if port_bindings.is_empty() {
            None
        } else {
            Some(port_bindings)
        },
        extra_hosts: if config.extra_hosts.is_empty() {
            None
        } else {
            Some(config.extra_hosts.clone())
        },
        ..Default::default()
    };

    let container_config = Config {
        image: Some(config.image.clone()),
        env: Some(env),
        labels: Some(labels),
        exposed_ports: if exposed_ports.is_empty() {
            None
        } else {
            Some(exposed_ports)
        },
        host_config: Some(host_config),
        entrypoint: config.entrypoint.clone(),
        cmd: config.cmd.clone(),
        working_dir: config.working_dir.clone(),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: &container_name,
        platform: None,
    };

    let response = docker
        .create_container(Some(options), container_config)
        .await
        .map_err(|e| anyhow::anyhow!("create container failed: {e}"))?;

    let container_id = response.id;
    info!(id = %container_id, "container created, starting...");

    docker
        .start_container::<String>(&container_id, None)
        .await
        .map_err(|e| anyhow::anyhow!("start container failed: {e}"))?;

    info!(id = %container_id, "container started");

    Ok(container_id)
}

/// Stop a container.
pub async fn stop_container(state: &RemoteState, container_id: &str) -> anyhow::Result<()> {
    state
        .docker
        .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
        .await
        .map_err(|e| anyhow::anyhow!("stop failed: {e}"))?;
    info!(id = %container_id, "container stopped");
    Ok(())
}

/// Remove a container.
pub async fn remove_container(state: &RemoteState, container_id: &str) -> anyhow::Result<()> {
    state
        .docker
        .remove_container(
            container_id,
            Some(RemoveContainerOptions {
                force: true,
                v: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| anyhow::anyhow!("remove failed: {e}"))?;
    info!(id = %container_id, "container removed");
    Ok(())
}

/// Execute a command in a container and return the output.
pub async fn exec_in_container(
    state: &RemoteState,
    container_id: &str,
    cmd: Vec<String>,
) -> anyhow::Result<ExecResult> {
    let docker = &state.docker;

    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                cmd: Some(cmd.clone()),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("create exec failed: {e}"))?;

    let output = docker
        .start_exec(
            &exec.id,
            Some(StartExecOptions {
                detach: false,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| anyhow::anyhow!("start exec failed: {e}"))?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    if let bollard::exec::StartExecResults::Attached { mut output, .. } = output {
        while let Some(Ok(chunk)) = output.next().await {
            match chunk {
                bollard::container::LogOutput::StdOut { message } => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
    }

    let inspect = docker
        .inspect_exec(&exec.id)
        .await
        .map_err(|e| anyhow::anyhow!("inspect exec failed: {e}"))?;

    let exit_code = inspect.exit_code.unwrap_or(-1);

    Ok(ExecResult {
        exit_code,
        stdout,
        stderr,
    })
}

/// Get container IP address on the bridge network.
/// Checks both the top-level IPAddress and per-network IPs (needed for DinD).
pub async fn get_container_ip(
    state: &RemoteState,
    container_id: &str,
) -> anyhow::Result<String> {
    let info = state
        .docker
        .inspect_container(container_id, None)
        .await
        .map_err(|e| anyhow::anyhow!("inspect failed: {e}"))?;

    let ns = info.network_settings
        .ok_or_else(|| anyhow::anyhow!("no network settings"))?;

    // Try top-level IPAddress first
    if let Some(ref ip) = ns.ip_address {
        if !ip.is_empty() {
            return Ok(ip.clone());
        }
    }

    // Fallback: check per-network IPs (e.g. bridge network)
    if let Some(ref networks) = ns.networks {
        for (_name, network) in networks {
            if let Some(ref ip) = network.ip_address {
                if !ip.is_empty() {
                    return Ok(ip.clone());
                }
            }
        }
    }

    anyhow::bail!("container has no IP address")
}
