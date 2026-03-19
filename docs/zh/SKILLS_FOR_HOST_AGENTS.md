# 主机代理的技能

如果你在使用 Coasts 的项目中使用 AI 编码代理（Claude Code、Codex、Conductor、Cursor 或类似工具），你的代理需要一个技能，用来教它如何与 Coast 运行时交互。没有这个技能，代理虽然会编辑文件，但不会知道如何运行测试、检查日志，或验证它的更改是否在运行中的环境内生效。

本指南将逐步介绍如何设置该技能。

## 为什么代理需要这个

Coasts 在你的主机与 Coast 容器之间共享[文件系统](concepts_and_terminology/FILESYSTEM.md)。你的代理在主机上编辑文件，而 Coast 内运行的服务会立即看到这些更改。但代理仍然需要:

1. **发现它正在使用哪个 Coast 实例** — `coast lookup` 会根据代理当前目录解析这个信息。
2. **在 Coast 内运行命令** — 测试、构建和其他运行时任务都通过 `coast exec` 在容器内执行。
3. **读取日志并检查服务状态** — `coast logs` 和 `coast ps` 为代理提供运行时反馈。

下面的技能会将这三点都教给代理。

## 技能

将以下内容添加到你的代理现有技能、规则或提示文件中。如果你的代理已经有关于运行测试或与你的开发环境交互的说明，这部分内容应与它们放在一起——它会教代理如何使用 Coasts 执行运行时操作。

```text-copy
This project uses Coasts (containerized host) for isolated development environments.
Your code edits are automatically visible inside the running Coast — the filesystem
is shared between the host and the container.

=== ORIENTATION ===

Before running any runtime commands, discover which Coast instance matches your
current working directory:

  coast lookup

This prints the instance name, ports, URLs, and example commands. Use the instance
name from the output for all subsequent commands.

If you need deeper context on how Coasts work, read these docs:

  coast docs --path concepts_and_terminology/LOOKUP.md
  coast docs --path concepts_and_terminology/FILESYSTEM.md
  coast docs --path concepts_and_terminology/EXEC_AND_DOCKER.md
  coast docs --path concepts_and_terminology/LOGS.md

=== RUNNING COMMANDS ===

Use `coast exec` to run commands inside the Coast. The shell starts at the workspace
root (where the Coastfile is). cd to your target directory first:

  coast exec <instance> -- sh -c "cd <dir> && <command>"

Examples:

  coast exec dev-1 -- sh -c "cd src && npm test"
  coast exec dev-1 -- sh -c "cd backend && go test ./..."
  coast exec dev-1 -- sh -c "cd apps/web && npx playwright test"

=== RUNTIME FEEDBACK ===

Check service status:

  coast ps <instance>

Read service logs:

  coast logs <instance> --service <service>
  coast logs <instance> --service <service> --tail 50

=== TROUBLESHOOTING ===

If you encounter errors or unfamiliar behavior, search the Coast docs:

  coast search-docs "error message or description"

This uses semantic search — describe the problem in natural language and it will
find the relevant documentation.

=== WORKTREE AWARENESS ===

When you start working in a worktree — whether you created it or a tool like
Codex, Conductor, or T3 Code created it for you — check if a Coast instance is
already assigned:

  coast lookup

If `coast lookup` finds an instance, use it for all runtime commands.

If it returns no instances, check what's currently running:

  coast ls

Then ask the user which option they prefer:

Option 1 — Create a new Coast and assign this worktree:
  coast run <new-name>
  coast assign <new-name> -w <worktree>

Option 2 — Reassign an existing Coast to this worktree:
  coast assign <existing-name> -w <worktree>

Option 3 — Skip Coast entirely:
Continue without a runtime environment. You can edit files but cannot run tests,
builds, or services inside a container.

The <worktree> value is the branch name (run `git branch --show-current`) or
the worktree identifier shown in `coast ls`. Always ask the user before creating
or reassigning — do not do it automatically.

=== RULES ===

- Always run `coast lookup` before your first runtime command in a session.
- Do not run services directly on the host. Use `coast exec` for all runtime tasks.
- File edits on the host are instantly visible inside the Coast. You do not need
  to copy files or rebuild after editing.
- If `coast lookup` returns no instances, the Coast may not be running. Follow the
  worktree awareness flow above to resolve this with the user.
```

## 将技能添加到你的代理

最快的方法是让代理自己完成设置。将下面的提示复制到你的代理聊天中——它包含技能文本，以及指导代理将其写入自身配置文件（`CLAUDE.md`、`AGENTS.md`、`.cursor/rules/coast.md` 等）的说明。

```prompt-copy
skills_prompt.txt
```

你也可以通过运行 `coast skills-prompt` 从 CLI 获取相同的输出。

### 手动设置

如果你更愿意自己添加这个技能:

- **Claude Code:** 将技能文本添加到你项目的 `CLAUDE.md` 文件中。
- **Codex:** 将技能文本添加到你项目的 `AGENTS.md` 文件中。
- **Cursor:** 在项目根目录创建 `.cursor/rules/coast.md` 并粘贴技能文本。
- **其他代理:** 将技能文本粘贴到你的代理在启动时读取的任何项目级提示或规则文件中。

## 延伸阅读

- 阅读 [Coastfiles 文档](coastfiles/README.md) 以了解完整的配置模式
- 学习用于管理实例的 [Coast CLI](concepts_and_terminology/CLI.md) 命令
- 探索 [Coastguard](concepts_and_terminology/COASTGUARD.md)，这是用于观察和控制你的 Coasts 的 Web UI
- 浏览 [概念与术语](concepts_and_terminology/README.md)，全面了解 Coasts 的工作方式
