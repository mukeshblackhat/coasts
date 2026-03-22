/// Shared compose command resolution helpers used by the CLI and daemon.
use std::path::Path;

/// Compose args needed to address the inner compose stack.
/// Mirrors the logic used when Coast starts the stack.
#[derive(Debug, Clone)]
pub struct ComposeContext {
    pub project_name: String,
    pub compose_rel_dir: Option<String>,
}

impl ComposeContext {
    fn project_dir(&self) -> String {
        match &self.compose_rel_dir {
            Some(dir) => format!("/workspace/{dir}"),
            None => "/workspace".to_string(),
        }
    }

    /// Build a shell script that discovers the compose file at runtime
    /// inside the container and runs the given docker compose subcommand.
    ///
    /// Checks paths in priority order:
    /// 1. `/coast-override/docker-compose.coast.yml`
    /// 2. `/coast-artifact/compose.yml`
    /// 3. `<project_dir>/docker-compose.yml`
    /// 4. `<project_dir>/docker-compose.yaml`
    /// 5. `/workspace/docker-compose.yml`
    pub fn compose_script(&self, subcmd: &str) -> String {
        let project_dir = self.project_dir();
        format!(
            concat!(
                "if [ -f /coast-override/docker-compose.coast.yml ]; then ",
                "  docker compose -p {proj} -f /coast-override/docker-compose.coast.yml --project-directory {dir} {subcmd}; ",
                "elif [ -f /coast-artifact/compose.yml ]; then ",
                "  docker compose -p {proj} -f /coast-artifact/compose.yml --project-directory {dir} {subcmd}; ",
                "elif [ -f {dir}/docker-compose.yml ]; then ",
                "  docker compose -p {proj} -f {dir}/docker-compose.yml --project-directory {dir} {subcmd}; ",
                "elif [ -f {dir}/docker-compose.yaml ]; then ",
                "  docker compose -p {proj} -f {dir}/docker-compose.yaml --project-directory {dir} {subcmd}; ",
                "elif [ -f /workspace/docker-compose.yml ]; then ",
                "  docker compose -p {proj} -f /workspace/docker-compose.yml {subcmd}; ",
                "else ",
                "  echo 'no compose file found' >&2; exit 1; ",
                "fi",
            ),
            proj = self.project_name,
            dir = project_dir,
            subcmd = subcmd,
        )
    }

    /// Build a `sh -c` command that runs the given docker compose subcommand.
    pub fn compose_shell(&self, subcmd: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), self.compose_script(subcmd)]
    }
}

/// Derive compose context for a Coast project by reading the stored Coastfile.
pub fn compose_context(project: &str) -> ComposeContext {
    compose_context_for_build(project, None)
}

/// Like [`compose_context`] but resolves the coastfile from a specific build.
pub fn compose_context_for_build(project: &str, build_id: Option<&str>) -> ComposeContext {
    let home = dirs::home_dir().unwrap_or_default();
    let project_dir = home.join(".coast").join("images").join(project);
    let coastfile_path = match build_id {
        Some(bid) => {
            let p = project_dir.join(bid).join("coastfile.toml");
            if p.exists() {
                p
            } else {
                project_dir.join("latest").join("coastfile.toml")
            }
        }
        None => project_dir.join("latest").join("coastfile.toml"),
    };

    let compose_rel_dir = if coastfile_path.exists() {
        std::fs::read_to_string(&coastfile_path)
            .ok()
            .and_then(|text| {
                let raw: toml::Value = text.parse().ok()?;
                let compose_str = raw.get("coast")?.get("compose")?.as_str()?;
                let compose_path = Path::new(compose_str);
                let parent = compose_path.parent()?;
                let dir_name = parent.file_name()?.to_str()?;
                Some(dir_name.to_string())
            })
    } else {
        None
    };

    let project_name = compose_rel_dir
        .clone()
        .unwrap_or_else(|| format!("coast-{project}"));

    ComposeContext {
        project_name,
        compose_rel_dir,
    }
}

/// Escape a string for safe single-quoted shell use.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Join argv-style command parts into a shell-safe string.
pub fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_shell_with_subdir() {
        let ctx = ComposeContext {
            project_name: "infra".into(),
            compose_rel_dir: Some("infra".into()),
        };
        let cmd = ctx.compose_shell("ps --format json");
        assert_eq!(cmd[0], "sh");
        assert_eq!(cmd[1], "-c");
        assert!(cmd[2].contains("-p infra"));
        assert!(cmd[2].contains("/coast-artifact/compose.yml"));
        assert!(cmd[2].contains("/workspace/infra/docker-compose.yml"));
        assert!(cmd[2].contains("ps --format json"));
    }

    #[test]
    fn test_compose_shell_no_subdir() {
        let ctx = ComposeContext {
            project_name: "coast-myapp".into(),
            compose_rel_dir: None,
        };
        let cmd = ctx.compose_shell("logs --tail 200");
        assert!(cmd[2].contains("-p coast-myapp"));
        assert!(cmd[2].contains("/workspace/docker-compose.yml"));
        assert!(cmd[2].contains("logs --tail 200"));
    }

    #[test]
    fn test_compose_context_root_level_compose_uses_default_project_name() {
        let dir = tempfile::tempdir().unwrap();
        let coastfile = dir.path().join("coastfile.toml");
        std::fs::write(
            &coastfile,
            r#"
[coast]
name = "my-app"
compose = "./docker-compose.yml"
"#,
        )
        .unwrap();

        let text = std::fs::read_to_string(&coastfile).unwrap();
        let raw: toml::Value = text.parse().unwrap();
        let compose_str = raw
            .get("coast")
            .and_then(|c| c.get("compose"))
            .and_then(|v| v.as_str())
            .unwrap();
        let compose_path = Path::new(compose_str);
        let parent = compose_path.parent().unwrap();
        let dir_name = parent.file_name().and_then(|f| f.to_str());
        assert!(dir_name.is_none());
    }

    #[test]
    fn test_shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("echo 'hi'"), "'echo '\\''hi'\\'''");
    }

    #[test]
    fn test_shell_join_quotes_each_part() {
        let joined = shell_join(&["echo".to_string(), "hello world".to_string()]);
        assert_eq!(joined, "'echo' 'hello world'");
    }
}
