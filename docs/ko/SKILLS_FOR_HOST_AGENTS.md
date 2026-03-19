# 호스트 에이전트를 위한 스킬

Coasts를 사용하는 프로젝트에서 AI 코딩 에이전트(Claude Code, Codex, Conductor, Cursor 또는 유사한 도구)를 사용하고 있다면, 에이전트가 Coast 런타임과 상호작용하는 방법을 가르치는 스킬이 필요합니다. 이것이 없으면 에이전트는 파일을 편집할 수는 있지만, 테스트를 실행하고, 로그를 확인하고, 실행 중인 환경 내부에서 변경 사항이 제대로 동작하는지 검증하는 방법은 알지 못합니다.

이 가이드는 해당 스킬을 설정하는 과정을 안내합니다.

## 에이전트에 이것이 필요한 이유

Coasts는 호스트 머신과 Coast 컨테이너 사이에서 [filesystem](concepts_and_terminology/FILESYSTEM.md)을 공유합니다. 에이전트는 호스트에서 파일을 편집하고, Coast 내부에서 실행 중인 서비스는 그 변경 사항을 즉시 확인합니다. 하지만 에이전트는 여전히 다음을 해야 합니다:

1. **어떤 Coast 인스턴스로 작업 중인지 파악하기** — `coast lookup` 이 에이전트의 현재 디렉터리를 기준으로 이를 확인합니다.
2. **Coast 내부에서 명령 실행하기** — 테스트, 빌드 및 기타 런타임 작업은 컨테이너 내부에서 `coast exec` 를 통해 수행됩니다.
3. **로그를 읽고 서비스 상태를 확인하기** — `coast logs` 와 `coast ps` 는 에이전트에 런타임 피드백을 제공합니다.

아래의 스킬은 에이전트에게 이 세 가지를 모두 가르칩니다.

## 스킬

다음을 에이전트의 기존 스킬, 규칙 또는 프롬프트 파일에 추가하세요. 에이전트에 이미 테스트 실행이나 개발 환경과의 상호작용에 대한 지침이 있다면, 이것은 그 지침과 함께 들어가야 합니다 — 런타임 작업에 Coasts를 사용하는 방법을 에이전트에게 가르치기 때문입니다.

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

## 에이전트에 스킬 추가하기

가장 빠른 방법은 에이전트가 스스로 설정하게 하는 것입니다. 아래 프롬프트를 에이전트의 채팅에 복사해 넣으세요 — 여기에는 스킬 텍스트와 함께, 에이전트가 그것을 자신의 설정 파일(`CLAUDE.md`, `AGENTS.md`, `.cursor/rules/coast.md` 등)에 쓰도록 하는 지침이 포함되어 있습니다.

```prompt-copy
skills_prompt.txt
```

CLI에서 `coast skills-prompt` 를 실행해도 동일한 출력을 얻을 수 있습니다.

### 수동 설정

직접 스킬을 추가하고 싶다면:

- **Claude Code:** 프로젝트의 `CLAUDE.md` 파일에 스킬 텍스트를 추가하세요.
- **Codex:** 프로젝트의 `AGENTS.md` 파일에 스킬 텍스트를 추가하세요.
- **Cursor:** 프로젝트 루트에 `.cursor/rules/coast.md` 를 만들고 스킬 텍스트를 붙여넣으세요.
- **기타 에이전트:** 에이전트가 시작 시 읽는 프로젝트 수준의 프롬프트 또는 규칙 파일에 스킬 텍스트를 붙여넣으세요.

## 추가 읽을거리

- 전체 구성 스키마를 배우려면 [Coastfiles documentation](coastfiles/README.md)을 읽어보세요
- 인스턴스를 관리하는 명령을 배우려면 [Coast CLI](concepts_and_terminology/CLI.md)를 확인하세요
- Coasts를 관찰하고 제어하는 웹 UI인 [Coastguard](concepts_and_terminology/COASTGUARD.md)를 살펴보세요
- Coasts의 작동 방식을 전체적으로 이해하려면 [Concepts & Terminology](concepts_and_terminology/README.md)를 둘러보세요
