use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::info;

use coast_docker::runtime::Runtime;

/// Collected secret injection data returned by [`load_secrets_for_instance`].
pub(super) struct SecretInjectionPlan {
    /// Environment variables to inject into the coast container.
    pub env_vars: std::collections::HashMap<String, String>,
    /// Bind mounts for file-type secrets (host path -> container path).
    pub bind_mounts: Vec<coast_docker::runtime::BindMount>,
    /// Container paths for secret files (used in compose overrides).
    pub container_paths: Vec<String>,
    /// File contents to write via exec after the DinD container starts.
    /// Bind mounts from host don't propagate through DinD's overlay correctly,
    /// so we write file secrets via exec instead.
    pub files_for_exec: Vec<(String, Vec<u8>)>,
}

/// Load secrets from the keystore and build an injection plan.
///
/// Reads the encrypted keystore, resolves all secrets for the given coastfile
/// image name, and produces env vars, bind mounts, and file data for exec injection.
pub(super) fn load_secrets_for_instance(
    coastfile_path: &Path,
    instance_name: &str,
) -> SecretInjectionPlan {
    let home = dirs::home_dir().unwrap_or_default();
    let (keystore_db, keystore_key) = keystore_paths(&home);
    if !keystore_is_available(&keystore_db, &keystore_key) {
        return empty_secret_injection_plan();
    }

    let Some(secret_scope) = load_declared_secret_scope(coastfile_path) else {
        return empty_secret_injection_plan();
    };

    let Some((env_vars, bind_mounts)) = load_secret_injection_parts(
        &home,
        &keystore_db,
        &keystore_key,
        &secret_scope,
        instance_name,
    ) else {
        return empty_secret_injection_plan();
    };

    secret_plan_from_parts(env_vars, bind_mounts)
}

struct DeclaredSecretScope {
    image_name: String,
    declared_names: HashSet<String>,
}

fn keystore_paths(home: &Path) -> (PathBuf, PathBuf) {
    (
        home.join(".coast").join("keystore.db"),
        home.join(".coast").join("keystore.key"),
    )
}

fn keystore_is_available(keystore_db: &Path, keystore_key: &Path) -> bool {
    keystore_db.exists() && keystore_key.exists()
}

fn load_declared_secret_scope(coastfile_path: &Path) -> Option<DeclaredSecretScope> {
    let coastfile = if coastfile_path.exists() {
        coast_core::coastfile::Coastfile::from_file(coastfile_path).ok()
    } else {
        None
    }?;

    Some(DeclaredSecretScope {
        image_name: coastfile.name.clone(),
        declared_names: coastfile
            .secrets
            .iter()
            .map(|secret| secret.name.clone())
            .collect(),
    })
}

fn load_secret_injection_parts(
    home: &Path,
    keystore_db: &Path,
    keystore_key: &Path,
    secret_scope: &DeclaredSecretScope,
    instance_name: &str,
) -> Option<(
    HashMap<String, String>,
    Vec<coast_docker::runtime::BindMount>,
)> {
    let resolved_secrets = load_resolved_secrets(keystore_db, keystore_key, secret_scope)?;
    let tmpfs_base = home
        .join(".coast")
        .join("secrets-tmpfs")
        .join(instance_name);

    let injection_plan =
        match coast_secrets::inject::build_injection_plan(&resolved_secrets, &tmpfs_base) {
            Ok(plan) => plan,
            Err(e) => {
                tracing::warn!(error = %e, "failed to build secret injection plan");
                return None;
            }
        };

    let bind_mounts =
        write_secret_file_mounts(&injection_plan.file_mounts, &resolved_secrets, &tmpfs_base);

    info!(
        env_count = injection_plan.env_vars.len(),
        file_count = bind_mounts.len(),
        "secrets loaded for injection"
    );

    Some((injection_plan.env_vars, bind_mounts))
}

fn load_resolved_secrets(
    keystore_db: &Path,
    keystore_key: &Path,
    secret_scope: &DeclaredSecretScope,
) -> Option<Vec<coast_secrets::inject::ResolvedSecret>> {
    match coast_secrets::keystore::Keystore::open(keystore_db, keystore_key) {
        Ok(keystore) => match keystore.get_all_secrets(&secret_scope.image_name) {
            Ok(secrets) if !secrets.is_empty() => Some(filter_declared_resolved_secrets(
                &secrets,
                &secret_scope.declared_names,
            )),
            Ok(_) => {
                info!("no secrets found in keystore for project");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to read secrets from keystore");
                None
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "failed to open keystore");
            None
        }
    }
}

fn filter_declared_resolved_secrets(
    secrets: &[coast_secrets::keystore::StoredSecret],
    declared_names: &HashSet<String>,
) -> Vec<coast_secrets::inject::ResolvedSecret> {
    secrets
        .iter()
        .filter(|secret| declared_names.contains(&secret.secret_name))
        .map(|secret| coast_secrets::inject::ResolvedSecret {
            name: secret.secret_name.clone(),
            inject_type: secret.inject_type.clone(),
            inject_target: secret.inject_target.clone(),
            value: secret.value.clone(),
        })
        .collect()
}

fn write_secret_file_mounts(
    file_mounts: &[coast_secrets::inject::FileMount],
    resolved_secrets: &[coast_secrets::inject::ResolvedSecret],
    tmpfs_base: &Path,
) -> Vec<coast_docker::runtime::BindMount> {
    if file_mounts.is_empty() {
        return Vec::new();
    }

    if let Err(e) = std::fs::create_dir_all(tmpfs_base) {
        tracing::warn!(error = %e, "failed to create secrets tmpfs dir");
        return Vec::new();
    }

    let mut bind_mounts = Vec::new();
    for file_mount in file_mounts {
        if let Some(bind_mount) = write_secret_file_mount(file_mount, resolved_secrets) {
            bind_mounts.push(bind_mount);
        }
    }
    bind_mounts
}

fn write_secret_file_mount(
    file_mount: &coast_secrets::inject::FileMount,
    resolved_secrets: &[coast_secrets::inject::ResolvedSecret],
) -> Option<coast_docker::runtime::BindMount> {
    let secret_name = file_mount
        .host_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let secret = resolved_secrets
        .iter()
        .find(|secret| secret.name == secret_name)?;

    if let Err(e) = std::fs::write(&file_mount.host_path, &secret.value) {
        tracing::warn!(
            error = %e,
            path = %file_mount.host_path.display(),
            "failed to write secret file"
        );
        return None;
    }

    Some(coast_docker::runtime::BindMount {
        host_path: file_mount.host_path.clone(),
        container_path: file_mount.container_path.to_string_lossy().to_string(),
        read_only: false,
        propagation: None,
    })
}

fn secret_plan_from_parts(
    env_vars: HashMap<String, String>,
    bind_mounts: Vec<coast_docker::runtime::BindMount>,
) -> SecretInjectionPlan {
    let container_paths: Vec<String> = bind_mounts
        .iter()
        .map(|bind_mount| bind_mount.container_path.clone())
        .collect();
    let files_for_exec: Vec<(String, Vec<u8>)> = bind_mounts
        .iter()
        .filter_map(|bind_mount| {
            std::fs::read(&bind_mount.host_path)
                .ok()
                .map(|data| (bind_mount.container_path.clone(), data))
        })
        .collect();

    SecretInjectionPlan {
        env_vars,
        bind_mounts,
        container_paths,
        files_for_exec,
    }
}

fn empty_secret_injection_plan() -> SecretInjectionPlan {
    secret_plan_from_parts(HashMap::new(), Vec::new())
}

/// Write file-type secrets directly into the DinD container via exec.
///
/// Bind mounts from host don't propagate through DinD's overlay to the inner
/// Docker daemon, so inner compose services see a directory instead of a file.
/// Writing via exec creates the file on the DinD overlay where the inner daemon
/// can bind-mount it into service containers.
pub(super) async fn write_secret_files_via_exec(
    files_for_exec: &[(String, Vec<u8>)],
    container_id: &str,
    docker: &bollard::Docker,
) {
    if files_for_exec.is_empty() {
        return;
    }
    let runtime = coast_docker::dind::DindRuntime::with_client(docker.clone());
    for (container_path, data) in files_for_exec {
        let parent = std::path::Path::new(container_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        let b64 = base64_encode_bytes(data);
        let cmd = format!(
            "mkdir -p '{}' && echo '{}' | base64 -d > '{}'",
            parent, b64, container_path
        );
        if let Err(e) = runtime
            .exec_in_coast(container_id, &["sh", "-c", &cmd])
            .await
        {
            tracing::warn!(
                error = %e,
                path = %container_path,
                "failed to write secret file via exec"
            );
        }
    }
    info!(
        count = files_for_exec.len(),
        "secret files written into DinD via exec"
    );
}

/// Base64-encode a byte slice using the standard alphabet.
fn base64_encode_bytes(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[((b0 & 0x03) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((b1 & 0x0f) << 2 | b2 >> 6) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn stored_secret(
        name: &str,
        inject_type: &str,
        inject_target: &str,
        value: &[u8],
    ) -> coast_secrets::keystore::StoredSecret {
        coast_secrets::keystore::StoredSecret {
            coast_image: "test-image".to_string(),
            secret_name: name.to_string(),
            value: value.to_vec(),
            inject_type: inject_type.to_string(),
            inject_target: inject_target.to_string(),
            extracted_at: Utc::now(),
            extractor: "test".to_string(),
            ttl_seconds: None,
        }
    }

    // --- base64_encode_bytes ---

    #[test]
    fn test_base64_encode_empty() {
        assert_eq!(base64_encode_bytes(b""), "");
    }

    #[test]
    fn test_base64_encode_hello() {
        assert_eq!(base64_encode_bytes(b"Hello"), "SGVsbG8=");
    }

    #[test]
    fn test_base64_encode_padding_two_bytes() {
        assert_eq!(base64_encode_bytes(b"ab"), "YWI=");
    }

    #[test]
    fn test_base64_encode_no_padding() {
        assert_eq!(base64_encode_bytes(b"abc"), "YWJj");
    }

    #[test]
    fn test_base64_encode_single_byte() {
        assert_eq!(base64_encode_bytes(b"a"), "YQ==");
    }

    #[test]
    fn test_base64_encode_binary_data() {
        assert_eq!(base64_encode_bytes(&[0x00, 0xFF, 0x80]), "AP+A");
    }

    #[test]
    fn test_base64_encode_longer_string() {
        assert_eq!(
            base64_encode_bytes(b"Hello, World!"),
            "SGVsbG8sIFdvcmxkIQ=="
        );
    }

    // --- load_secrets_for_instance ---

    #[test]
    fn test_load_secrets_nonexistent_coastfile_returns_empty_plan() {
        let plan =
            load_secrets_for_instance(Path::new("/nonexistent/coastfile.toml"), "test-instance");
        assert!(plan.env_vars.is_empty());
        assert!(plan.bind_mounts.is_empty());
        assert!(plan.container_paths.is_empty());
        assert!(plan.files_for_exec.is_empty());
    }

    #[test]
    fn test_load_secrets_no_keystore_returns_empty_plan() {
        let dir = tempfile::tempdir().unwrap();
        let coastfile_path = dir.path().join("coastfile.toml");
        std::fs::write(&coastfile_path, "name = \"test\"\n").unwrap();

        let plan = load_secrets_for_instance(&coastfile_path, "test-instance");
        assert!(plan.env_vars.is_empty());
        assert!(plan.bind_mounts.is_empty());
        assert!(plan.container_paths.is_empty());
        assert!(plan.files_for_exec.is_empty());
    }

    #[test]
    fn test_secret_injection_plan_container_paths_match_bind_mounts() {
        let plan = SecretInjectionPlan {
            env_vars: std::collections::HashMap::new(),
            bind_mounts: vec![
                coast_docker::runtime::BindMount {
                    host_path: std::path::PathBuf::from("/tmp/secret1"),
                    container_path: "/run/secrets/db_pass".to_string(),
                    read_only: false,
                    propagation: None,
                },
                coast_docker::runtime::BindMount {
                    host_path: std::path::PathBuf::from("/tmp/secret2"),
                    container_path: "/run/secrets/api_key".to_string(),
                    read_only: false,
                    propagation: None,
                },
            ],
            container_paths: vec![
                "/run/secrets/db_pass".to_string(),
                "/run/secrets/api_key".to_string(),
            ],
            files_for_exec: vec![],
        };
        assert_eq!(plan.container_paths.len(), plan.bind_mounts.len());
        for (bm, cp) in plan.bind_mounts.iter().zip(plan.container_paths.iter()) {
            assert_eq!(&bm.container_path, cp);
        }
    }

    #[test]
    fn test_filter_declared_resolved_secrets_keeps_only_declared_names() {
        let secrets = vec![
            stored_secret("api_key", "env", "API_KEY", b"secret123"),
            stored_secret(
                "gcp_creds",
                "file",
                "/run/secrets/gcp.json",
                b"{\"key\":\"value\"}",
            ),
        ];
        let declared_names = ["api_key".to_string()]
            .into_iter()
            .collect::<std::collections::HashSet<_>>();

        let resolved = filter_declared_resolved_secrets(&secrets, &declared_names);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "api_key");
        assert_eq!(resolved[0].inject_type, "env");
        assert_eq!(resolved[0].inject_target, "API_KEY");
        assert_eq!(resolved[0].value, b"secret123");
    }

    #[test]
    fn test_write_secret_file_mounts_writes_files_and_returns_bind_mounts() {
        let dir = tempfile::tempdir().unwrap();
        let file_mounts = vec![coast_secrets::inject::FileMount {
            host_path: dir.path().join("gcp_creds"),
            container_path: PathBuf::from("/run/secrets/gcp.json"),
        }];
        let resolved_secrets = vec![coast_secrets::inject::ResolvedSecret {
            name: "gcp_creds".to_string(),
            inject_type: "file".to_string(),
            inject_target: "/run/secrets/gcp.json".to_string(),
            value: b"{\"key\":\"value\"}".to_vec(),
        }];

        let bind_mounts = write_secret_file_mounts(&file_mounts, &resolved_secrets, dir.path());

        assert_eq!(bind_mounts.len(), 1);
        assert_eq!(bind_mounts[0].host_path, dir.path().join("gcp_creds"));
        assert_eq!(bind_mounts[0].container_path, "/run/secrets/gcp.json");
        assert_eq!(
            std::fs::read(dir.path().join("gcp_creds")).unwrap(),
            b"{\"key\":\"value\"}"
        );
    }

    #[test]
    fn test_secret_plan_from_parts_skips_missing_exec_files() {
        let dir = tempfile::tempdir().unwrap();
        let existing_host_path = dir.path().join("secret1");
        std::fs::write(&existing_host_path, b"secret-one").unwrap();

        let bind_mounts = vec![
            coast_docker::runtime::BindMount {
                host_path: existing_host_path.clone(),
                container_path: "/run/secrets/secret1".to_string(),
                read_only: false,
                propagation: None,
            },
            coast_docker::runtime::BindMount {
                host_path: dir.path().join("missing"),
                container_path: "/run/secrets/missing".to_string(),
                read_only: false,
                propagation: None,
            },
        ];

        let plan = secret_plan_from_parts(HashMap::new(), bind_mounts);

        assert_eq!(
            plan.container_paths,
            vec![
                "/run/secrets/secret1".to_string(),
                "/run/secrets/missing".to_string()
            ]
        );
        assert_eq!(plan.files_for_exec.len(), 1);
        assert_eq!(plan.files_for_exec[0].0, "/run/secrets/secret1");
        assert_eq!(plan.files_for_exec[0].1, b"secret-one");
    }
}
