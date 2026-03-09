use tracing::{info, warn};

use coast_core::error::Result;
use coast_core::protocol::BuildProgressEvent;
use coast_core::types::{McpClientConnectorConfig, McpClientFormat, McpServerConfig};
use coast_docker::dind::DindRuntime;
use coast_docker::runtime::Runtime;

use super::emit;

/// Install internal MCP servers and generate client configs inside the coast container.
pub(super) async fn install_mcp_servers(
    container_id: &str,
    mcp_servers: &[McpServerConfig],
    mcp_clients: &[McpClientConnectorConfig],
    docker: &bollard::Docker,
    progress: &tokio::sync::mpsc::Sender<BuildProgressEvent>,
) -> Result<()> {
    let runtime = DindRuntime::with_client(docker.clone());

    for server in mcp_servers {
        install_internal_mcp_server(container_id, server, &runtime, progress).await;
    }

    write_mcp_client_configs(container_id, mcp_servers, mcp_clients, &runtime, progress).await;

    Ok(())
}

async fn install_internal_mcp_server(
    container_id: &str,
    server: &McpServerConfig,
    runtime: &DindRuntime,
    progress: &tokio::sync::mpsc::Sender<BuildProgressEvent>,
) {
    if server.is_host_proxied() {
        return;
    }

    emit(
        progress,
        BuildProgressEvent::item("Installing MCP", &server.name, "started"),
    );

    create_mcp_server_dir(container_id, &server.name, runtime).await;
    copy_mcp_server_source(container_id, server, runtime).await;
    run_mcp_install_commands(container_id, server, runtime, progress).await;

    info!(server = %server.name, "MCP server installed at /mcp/{}", server.name);
}

async fn create_mcp_server_dir(container_id: &str, server_name: &str, runtime: &DindRuntime) {
    let mkdir_cmd = mcp_server_mkdir_command(server_name);
    let _ = runtime
        .exec_in_coast(container_id, &["sh", "-c", &mkdir_cmd])
        .await;
}

async fn copy_mcp_server_source(
    container_id: &str,
    server: &McpServerConfig,
    runtime: &DindRuntime,
) {
    let Some(source) = server.source.as_deref() else {
        return;
    };

    let copy_cmd = mcp_server_copy_command(source, &server.name);
    let copy_result = runtime
        .exec_in_coast(container_id, &["sh", "-c", &copy_cmd])
        .await;
    log_mcp_source_copy_failure(&server.name, copy_result);
}

fn log_mcp_source_copy_failure(
    server_name: &str,
    copy_result: Result<coast_docker::runtime::ExecResult>,
) {
    match copy_result {
        Ok(result) if !result.success() => {
            warn!(
                server = %server_name,
                stderr = %result.stderr,
                "MCP source copy failed"
            );
        }
        Err(error) => {
            warn!(server = %server_name, error = %error, "MCP source copy failed");
        }
        _ => {}
    }
}

async fn run_mcp_install_commands(
    container_id: &str,
    server: &McpServerConfig,
    runtime: &DindRuntime,
    progress: &tokio::sync::mpsc::Sender<BuildProgressEvent>,
) {
    for install_cmd in &server.install {
        run_mcp_install_command(container_id, &server.name, install_cmd, runtime, progress).await;
    }
}

async fn run_mcp_install_command(
    container_id: &str,
    server_name: &str,
    install_cmd: &str,
    runtime: &DindRuntime,
    progress: &tokio::sync::mpsc::Sender<BuildProgressEvent>,
) {
    emit(
        progress,
        BuildProgressEvent::item("Installing MCP", install_cmd, "started"),
    );

    let full_cmd = mcp_server_install_command(server_name, install_cmd);
    let install_result = runtime
        .exec_in_coast(container_id, &["sh", "-c", &full_cmd])
        .await;
    log_mcp_install_failure(server_name, install_cmd, install_result);
}

fn log_mcp_install_failure(
    server_name: &str,
    install_cmd: &str,
    install_result: Result<coast_docker::runtime::ExecResult>,
) {
    match install_result {
        Ok(result) if !result.success() => {
            warn!(
                server = %server_name,
                cmd = %install_cmd,
                stderr = %result.stderr,
                exit_code = result.exit_code,
                "MCP install command failed"
            );
        }
        Err(error) => {
            warn!(
                server = %server_name,
                cmd = %install_cmd,
                error = %error,
                "MCP install command failed"
            );
        }
        _ => {}
    }
}

async fn write_mcp_client_configs(
    container_id: &str,
    mcp_servers: &[McpServerConfig],
    mcp_clients: &[McpClientConnectorConfig],
    runtime: &DindRuntime,
    progress: &tokio::sync::mpsc::Sender<BuildProgressEvent>,
) {
    if mcp_clients.is_empty() || mcp_servers.is_empty() {
        return;
    }

    emit(
        progress,
        BuildProgressEvent::item("Installing MCP", "Writing client configs", "started"),
    );

    for client in mcp_clients {
        install_mcp_client(container_id, client, mcp_servers, runtime).await;
    }
}

async fn install_mcp_client(
    container_id: &str,
    client: &McpClientConnectorConfig,
    mcp_servers: &[McpServerConfig],
    runtime: &DindRuntime,
) {
    if let Some(format) = client.format.as_ref() {
        write_generated_mcp_client_config(container_id, client, format, mcp_servers, runtime).await;
        return;
    }

    if let Some(run_cmd) = client.run.as_deref() {
        pipe_mcp_client_manifest(container_id, run_cmd, mcp_servers, runtime).await;
    }
}

async fn write_generated_mcp_client_config(
    container_id: &str,
    client: &McpClientConnectorConfig,
    format: &McpClientFormat,
    mcp_servers: &[McpServerConfig],
    runtime: &DindRuntime,
) {
    let config_json = super::super::mcp::generate_mcp_client_config(mcp_servers, format);
    let config_path = client
        .resolved_config_path()
        .unwrap_or(format.default_config_path());
    let write_cmd = mcp_client_config_write_command(config_path, &config_json);
    let write_result = runtime
        .exec_in_coast(container_id, &["sh", "-c", &write_cmd])
        .await;

    log_mcp_client_config_write_result(&client.name, config_path, write_result);
}

fn log_mcp_client_config_write_result(
    client_name: &str,
    config_path: &str,
    write_result: Result<coast_docker::runtime::ExecResult>,
) {
    match write_result {
        Ok(result) if result.success() => {
            info!(
                client = %client_name,
                path = %config_path,
                "MCP client config written"
            );
        }
        Ok(result) => {
            warn!(
                client = %client_name,
                stderr = %result.stderr,
                "Failed to write MCP client config"
            );
        }
        Err(error) => {
            warn!(
                client = %client_name,
                error = %error,
                "Failed to write MCP client config"
            );
        }
    }
}

async fn pipe_mcp_client_manifest(
    container_id: &str,
    run_cmd: &str,
    mcp_servers: &[McpServerConfig],
    runtime: &DindRuntime,
) {
    let manifest =
        super::super::mcp::generate_mcp_client_config(mcp_servers, &McpClientFormat::ClaudeCode);
    let pipe_cmd = mcp_client_manifest_pipe_command(run_cmd, &manifest);
    let _ = runtime
        .exec_in_coast(container_id, &["sh", "-c", &pipe_cmd])
        .await;
}

fn mcp_server_mkdir_command(server_name: &str) -> String {
    format!("mkdir -p /mcp/{server_name}")
}

fn mcp_server_copy_command(source: &str, server_name: &str) -> String {
    format!(
        "cp -a /workspace/{}/.  /mcp/{server_name}/",
        source.trim_start_matches("./"),
    )
}

fn mcp_server_install_command(server_name: &str, install_cmd: &str) -> String {
    format!("cd /mcp/{server_name} && {install_cmd}")
}

fn mcp_client_config_write_command(config_path: &str, config_json: &str) -> String {
    let parent_dir = std::path::Path::new(config_path)
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();

    format!(
        "mkdir -p '{parent}' && cat > '{path}' << 'COAST_MCP_EOF'\n{json}\nCOAST_MCP_EOF",
        parent = parent_dir,
        path = config_path,
        json = config_json,
    )
}

fn mcp_client_manifest_pipe_command(run_cmd: &str, manifest: &str) -> String {
    format!(
        "cat << 'COAST_MCP_EOF' | {cmd}\n{json}\nCOAST_MCP_EOF",
        cmd = run_cmd,
        json = manifest,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::handlers::mcp;

    use super::*;
    use coast_core::types::{McpClientFormat, McpProxyMode};

    #[test]
    fn test_mcp_server_copy_command_trims_relative_prefix() {
        let command = mcp_server_copy_command("./tools/server", "search");
        assert_eq!(command, "cp -a /workspace/tools/server/.  /mcp/search/");
    }

    #[test]
    fn test_mcp_server_install_command_changes_into_server_dir() {
        let command = mcp_server_install_command("search", "npm install");
        assert_eq!(command, "cd /mcp/search && npm install");
    }

    #[test]
    fn test_mcp_client_config_write_command_creates_parent_and_writes_json() {
        let command = mcp_client_config_write_command(
            "/root/.config/mcp/config.json",
            "{\n  \"mcpServers\": {}\n}",
        );

        assert!(command.contains("mkdir -p '/root/.config/mcp'"));
        assert!(command.contains("cat > '/root/.config/mcp/config.json' << 'COAST_MCP_EOF'"));
        assert!(command.contains("{\n  \"mcpServers\": {}\n}"));
    }

    #[test]
    fn test_mcp_client_manifest_pipe_command_embeds_run_command_and_manifest() {
        let command = mcp_client_manifest_pipe_command("my-client --import", "{\"ok\":true}");
        assert_eq!(
            command,
            "cat << 'COAST_MCP_EOF' | my-client --import\n{\"ok\":true}\nCOAST_MCP_EOF"
        );
    }

    #[test]
    fn test_generated_cursor_client_config_uses_default_path() {
        let client = McpClientConnectorConfig {
            name: "cursor".to_string(),
            format: Some(McpClientFormat::Cursor),
            config_path: None,
            run: None,
        };
        let command = mcp_client_config_write_command(
            client
                .resolved_config_path()
                .unwrap_or(McpClientFormat::Cursor.default_config_path()),
            "{}",
        );

        assert!(command.contains("/workspace/.cursor/mcp.json"));
    }

    #[test]
    fn test_generate_manifest_includes_host_proxied_and_internal_servers() {
        let internal_server = McpServerConfig {
            name: "search".to_string(),
            proxy: None,
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            env: HashMap::new(),
            install: Vec::new(),
            source: Some("./tools/search".to_string()),
        };
        let host_server = McpServerConfig {
            name: "hosted".to_string(),
            proxy: Some(McpProxyMode::Host),
            command: Some("ignored".to_string()),
            args: vec!["ignored".to_string()],
            env: HashMap::new(),
            install: Vec::new(),
            source: None,
        };

        let manifest = mcp::generate_mcp_client_config(
            &[internal_server, host_server],
            &McpClientFormat::ClaudeCode,
        );

        assert!(manifest.contains("\"cwd\": \"/mcp/search/\""));
        assert!(manifest.contains("\"command\": \"coast-mcp-proxy\""));
        assert!(manifest.contains("\"hosted\""));
    }
}
