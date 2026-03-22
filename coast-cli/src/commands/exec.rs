/// `coast exec` command — execute a command inside a coast container.
///
/// Runs an arbitrary command inside the specified coast container.
/// When stdin is a TTY, spawns `docker exec -it` directly for full interactive
/// support. Otherwise, uses the daemon path to capture stdout/stderr as strings.
/// Defaults to `sh` if no command is given (Alpine-based coast containers may
/// not have `bash` installed).
use std::io::IsTerminal;

use anyhow::{bail, Result};
use clap::Args;

use coast_core::compose::{compose_context_for_build, shell_join, shell_quote};
use coast_core::protocol::{ExecRequest, Request, Response};

/// Arguments for `coast exec`.
#[derive(Debug, Args)]
pub struct ExecArgs {
    /// Name of the coast instance.
    pub name: String,

    /// Exec into a specific compose service container instead of the coast container.
    #[arg(long, value_name = "SERVICE")]
    pub service: Option<String>,

    /// Run as container root/default user instead of the host UID:GID mapping.
    #[arg(long)]
    pub root: bool,

    /// Command to run inside the coast container (default: sh).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

/// Build the container name from project and instance name.
pub fn container_name(project: &str, name: &str) -> String {
    format!("{}-coasts-{}", project, name)
}

/// Resolve host uid:gid for docker exec user mapping.
fn host_uid_gid() -> Option<String> {
    #[cfg(unix)]
    {
        let uid = unsafe { nix::libc::getuid() };
        let gid = unsafe { nix::libc::getgid() };
        Some(format!("{uid}:{gid}"))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Build arguments for interactive `docker exec`.
fn build_interactive_docker_exec_args(
    container: &str,
    command: &[String],
    user_spec: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "-it".to_string()];
    if let Some(user) = user_spec {
        args.push("-u".to_string());
        args.push(user.to_string());
    }
    args.push(container.to_string());
    args.extend(command.iter().cloned());
    args
}

fn build_service_exec_script(
    project: &str,
    build_id: Option<&str>,
    service: &str,
    command: &[String],
    user_spec: Option<&str>,
    interactive: bool,
) -> String {
    let ctx = compose_context_for_build(project, build_id);
    let resolve_service = ctx.compose_script(&format!("ps -q {}", shell_quote(service)));
    let user_flag = user_spec
        .map(|user| format!(" -u {}", shell_quote(user)))
        .unwrap_or_default();
    let tty_flag = if interactive { " -it" } else { "" };
    let inner_command = shell_join(command);
    let error_message = shell_quote(&format!("Service '{service}' is not running"));
    format!(
        "cid=\"$({resolve_service} | head -n1)\"; \
         if [ -z \"$cid\" ]; then echo {error_message} >&2; exit 1; fi; \
         exec docker exec{user_flag}{tty_flag} \"$cid\" {inner_command}"
    )
}

fn build_shell_command(script: &str) -> Vec<String> {
    vec!["sh".to_string(), "-c".to_string(), script.to_string()]
}

/// Resolve the command to run, defaulting to sh.
pub fn resolve_command(command: &[String]) -> Vec<String> {
    if command.is_empty() {
        vec!["sh".to_string()]
    } else {
        command.to_vec()
    }
}

/// Execute the `coast exec` command.
pub async fn execute(args: &ExecArgs, project: &str) -> Result<()> {
    let command = resolve_command(&args.command);

    // Interactive mode: stdin is a TTY → spawn docker exec -it directly
    // for full TTY passthrough without going through the daemon.
    if std::io::stdin().is_terminal() {
        let container = container_name(project, &args.name);
        let docker_args = if let Some(service) = &args.service {
            let build_id = super::resolve_instance_build_id(project, &args.name);
            let user_spec = if args.root { None } else { host_uid_gid() };
            let script = build_service_exec_script(
                project,
                build_id.as_deref(),
                service,
                &command,
                user_spec.as_deref(),
                true,
            );
            let shell_command = build_shell_command(&script);
            build_interactive_docker_exec_args(&container, &shell_command, None)
        } else {
            let user_spec = if args.root { None } else { host_uid_gid() };
            build_interactive_docker_exec_args(&container, &command, user_spec.as_deref())
        };
        let mut cmd = std::process::Command::new("docker");
        cmd.args(&docker_args);
        let status = cmd.status()?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        return Ok(());
    }

    // Non-interactive: use daemon path (captures stdout/stderr as strings)
    let request = Request::Exec(ExecRequest {
        name: args.name.clone(),
        project: project.to_string(),
        service: args.service.clone(),
        root: args.root,
        command,
    });

    let response = super::send_request(request).await?;

    match response {
        Response::Exec(resp) => {
            if !resp.stdout.is_empty() {
                print!("{}", resp.stdout);
            }
            if !resp.stderr.is_empty() {
                eprint!("{}", resp.stderr);
            }

            if resp.exit_code != 0 {
                std::process::exit(resp.exit_code);
            }

            Ok(())
        }
        Response::Error(e) => {
            bail!("{}", e.error);
        }
        _ => {
            bail!("Unexpected response from daemon");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(flatten)]
        args: ExecArgs,
    }

    #[test]
    fn test_exec_args_name_only() {
        let cli = TestCli::try_parse_from(["test", "feature-oauth"]).unwrap();
        assert_eq!(cli.args.name, "feature-oauth");
        assert!(cli.args.service.is_none());
        assert!(!cli.args.root);
        assert!(cli.args.command.is_empty());
    }

    #[test]
    fn test_exec_args_with_command() {
        let cli = TestCli::try_parse_from(["test", "feature-oauth", "ls", "-la"]).unwrap();
        assert_eq!(cli.args.name, "feature-oauth");
        assert_eq!(cli.args.command, vec!["ls", "-la"]);
    }

    #[test]
    fn test_exec_args_with_service_and_root() {
        let cli =
            TestCli::try_parse_from(["test", "feature-oauth", "--service", "web", "--root", "sh"])
                .unwrap();
        assert_eq!(cli.args.service.as_deref(), Some("web"));
        assert!(cli.args.root);
        assert_eq!(cli.args.command, vec!["sh"]);
    }

    #[test]
    fn test_exec_args_with_complex_command() {
        let cli =
            TestCli::try_parse_from(["test", "my-instance", "docker", "compose", "ps"]).unwrap();
        assert_eq!(cli.args.command, vec!["docker", "compose", "ps"]);
    }

    #[test]
    fn test_exec_args_missing_name() {
        let result = TestCli::try_parse_from(["test"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_command_empty_defaults_to_sh() {
        let cmd = resolve_command(&[]);
        assert_eq!(cmd, vec!["sh"]);
    }

    #[test]
    fn test_resolve_command_provided() {
        let cmd = resolve_command(&["ls".to_string(), "-la".to_string()]);
        assert_eq!(cmd, vec!["ls", "-la"]);
    }

    #[test]
    fn test_container_name_construction() {
        assert_eq!(container_name("my-app", "main"), "my-app-coasts-main");
        assert_eq!(
            container_name("my-app", "feature-oauth"),
            "my-app-coasts-feature-oauth"
        );
    }

    #[test]
    fn test_build_interactive_docker_exec_args_with_user() {
        let args = build_interactive_docker_exec_args(
            "my-app-coasts-main",
            &["sh".to_string(), "-c".to_string(), "echo hi".to_string()],
            Some("501:20"),
        );
        assert_eq!(
            args,
            vec![
                "exec",
                "-it",
                "-u",
                "501:20",
                "my-app-coasts-main",
                "sh",
                "-c",
                "echo hi",
            ]
        );
    }

    #[test]
    fn test_build_interactive_docker_exec_args_without_user() {
        let args =
            build_interactive_docker_exec_args("my-app-coasts-main", &["sh".to_string()], None);
        assert_eq!(args, vec!["exec", "-it", "my-app-coasts-main", "sh"]);
    }

    #[test]
    fn test_build_service_exec_script_uses_compose_lookup_and_inner_exec() {
        let script = build_service_exec_script(
            "my-app",
            None,
            "web",
            &["sh".to_string()],
            Some("501:20"),
            true,
        );
        assert!(script.contains("docker compose -p coast-my-app"));
        assert!(script.contains("ps -q 'web'"));
        assert!(script.contains("docker exec -u '501:20' -it"));
        assert!(script.contains("'sh'"));
    }
}
