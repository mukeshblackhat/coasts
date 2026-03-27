use std::collections::HashMap;
use std::path::PathBuf;

use bollard::Docker;
use tokio::sync::Mutex;
use tracing::info;

use coast_docker::host::connect_to_host_docker;

/// Tracked container on the remote host.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrackedContainer {
    pub container_id: String,
    pub project: String,
    pub instance_name: String,
    pub status: String,
}

/// Tracked SSHFS mount.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrackedMount {
    pub project: String,
    pub ssh_target: String,
    pub remote_path: String,
    pub mount_path: String,
}

/// Shared state for the remote agent.
pub struct RemoteState {
    /// Bollard Docker client (talks to the local Docker daemon on this remote machine).
    pub docker: Docker,
    /// Root directory where SSHFS mounts live (e.g., /mnt/coast).
    pub mount_dir: PathBuf,
    /// Containers managed by this remote agent, keyed by "{project}/{instance_name}".
    pub containers: Mutex<HashMap<String, TrackedContainer>>,
    /// Active SSHFS mounts, keyed by project name.
    pub mounts: Mutex<HashMap<String, TrackedMount>>,
}

impl RemoteState {
    pub async fn new(mount_dir: &str) -> anyhow::Result<Self> {
        let mount_dir = PathBuf::from(mount_dir);
        tokio::fs::create_dir_all(&mount_dir).await?;

        let docker = connect_to_host_docker()
            .map_err(|e| anyhow::anyhow!("Failed to connect to Docker: {e}"))?;

        // Verify Docker is reachable
        docker.ping().await.map_err(|e| {
            anyhow::anyhow!("Docker daemon not reachable: {e}")
        })?;
        info!("connected to Docker daemon");

        Ok(Self {
            docker,
            mount_dir,
            containers: Mutex::new(HashMap::new()),
            mounts: Mutex::new(HashMap::new()),
        })
    }

    /// Get the SSHFS mount path for a project.
    pub fn project_mount_path(&self, project: &str) -> PathBuf {
        self.mount_dir.join(project)
    }

    /// Container key for lookups.
    pub fn container_key(project: &str, instance: &str) -> String {
        format!("{project}/{instance}")
    }

    pub async fn track_container(
        &self,
        project: &str,
        instance: &str,
        container_id: &str,
        status: &str,
    ) {
        let key = Self::container_key(project, instance);
        self.containers.lock().await.insert(
            key,
            TrackedContainer {
                container_id: container_id.to_string(),
                project: project.to_string(),
                instance_name: instance.to_string(),
                status: status.to_string(),
            },
        );
    }

    pub async fn get_container(&self, project: &str, instance: &str) -> Option<TrackedContainer> {
        let key = Self::container_key(project, instance);
        self.containers.lock().await.get(&key).cloned()
    }

    pub async fn remove_tracked(&self, project: &str, instance: &str) {
        let key = Self::container_key(project, instance);
        self.containers.lock().await.remove(&key);
    }

    pub async fn list_containers(&self) -> Vec<TrackedContainer> {
        self.containers.lock().await.values().cloned().collect()
    }

    // --- Mount tracking ---

    pub async fn track_mount(&self, project: &str, ssh_target: &str, remote_path: &str) {
        let mount_path = self.project_mount_path(project);
        self.mounts.lock().await.insert(
            project.to_string(),
            TrackedMount {
                project: project.to_string(),
                ssh_target: ssh_target.to_string(),
                remote_path: remote_path.to_string(),
                mount_path: mount_path.to_string_lossy().to_string(),
            },
        );
    }

    pub async fn remove_mount(&self, project: &str) {
        self.mounts.lock().await.remove(project);
    }

    pub async fn list_mounts(&self) -> Vec<TrackedMount> {
        self.mounts.lock().await.values().cloned().collect()
    }

    pub async fn is_mounted(&self, project: &str) -> bool {
        self.mounts.lock().await.contains_key(project)
    }
}
