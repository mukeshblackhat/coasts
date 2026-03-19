# Навыки для хост-агентов

Если вы используете AI-агентов для программирования (Claude Code, Codex, Conductor, Cursor или аналогичные) в проекте, который использует Coasts, вашему агенту нужен навык, который научит его взаимодействовать со средой выполнения Coast. Без этого агент будет редактировать файлы, но не будет знать, как запускать тесты, проверять логи или убеждаться, что его изменения работают внутри запущенного окружения.

Это руководство поможет настроить такой навык.

## Зачем это нужно агентам

Coasts предоставляет общий [filesystem](concepts_and_terminology/FILESYSTEM.md) между вашей хост-машиной и контейнером Coast. Ваш агент редактирует файлы на хосте, а запущенные внутри Coast сервисы сразу видят эти изменения. Но агенту всё равно необходимо:

1. **Определить, с каким экземпляром Coast он работает** — `coast lookup` определяет это по текущему каталогу агента.
2. **Запускать команды внутри Coast** — тесты, сборки и другие задачи среды выполнения происходят внутри контейнера через `coast exec`.
3. **Читать логи и проверять статус сервисов** — `coast logs` и `coast ps` дают агенту обратную связь от среды выполнения.

Приведённый ниже навык обучает агента всем трём пунктам.

## Навык

Добавьте следующее в существующий файл навыков, правил или промпта вашего агента. Если у вашего агента уже есть инструкции по запуску тестов или взаимодействию с вашей dev-средой, это нужно разместить рядом с ними — этот текст объясняет агенту, как использовать Coasts для операций среды выполнения.

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

## Добавление навыка вашему агенту

Самый быстрый способ — позволить агенту настроить себя самостоятельно. Скопируйте приведённый ниже промпт в чат вашего агента — он включает текст навыка и инструкции для агента, чтобы тот записал его в собственный файл конфигурации (`CLAUDE.md`, `AGENTS.md`, `.cursor/rules/coast.md` и т. д.).

```prompt-copy
skills_prompt.txt
```

Вы также можете получить тот же результат через CLI, выполнив `coast skills-prompt`.

### Ручная настройка

Если вы предпочитаете добавить навык самостоятельно:

- **Claude Code:** Добавьте текст навыка в файл `CLAUDE.md` вашего проекта.
- **Codex:** Добавьте текст навыка в файл `AGENTS.md` вашего проекта.
- **Cursor:** Создайте `.cursor/rules/coast.md` в корне проекта и вставьте туда текст навыка.
- **Другие агенты:** Вставьте текст навыка в любой файл промпта или правил уровня проекта, который ваш агент читает при запуске.

## Дополнительное чтение

- Прочитайте [документацию Coastfiles](coastfiles/README.md), чтобы изучить полную схему конфигурации
- Изучите команды [Coast CLI](concepts_and_terminology/CLI.md) для управления экземплярами
- Ознакомьтесь с [Coastguard](concepts_and_terminology/COASTGUARD.md), веб-интерфейсом для наблюдения и управления вашими Coasts
- Просмотрите [Concepts & Terminology](concepts_and_terminology/README.md), чтобы получить полное представление о том, как работает Coasts
