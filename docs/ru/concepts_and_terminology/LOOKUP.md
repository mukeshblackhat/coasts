# Lookup

`coast lookup` обнаруживает, какие экземпляры Coast запущены для текущего рабочего каталога вызывающего. Это первая команда, которую должен запустить агент на стороне хоста, чтобы сориентироваться — «Я редактирую код здесь, с каким(и) Coast мне взаимодействовать?»

```bash
coast lookup
```

Lookup определяет, находитесь ли вы внутри [worktree](ASSIGN.md) или в корне проекта, запрашивает у демона соответствующие экземпляры и выводит результаты с портами, URL и примерами команд.

## Why This Exists

ИИ-агент для кодинга, работающий на хосте (Cursor, Claude Code, Codex и т. п.), редактирует файлы через [shared filesystem](FILESYSTEM.md) и вызывает команды Coast CLI для операций во время выполнения. Но сначала агенту нужно ответить на базовый вопрос: **какой экземпляр Coast соответствует каталогу, в котором я работаю?**

Без `coast lookup` агенту пришлось бы запустить `coast ls`, разобрать полную таблицу экземпляров, определить, в каком worktree он находится, и сопоставить это. `coast lookup` делает всё это за один шаг и возвращает структурированный вывод, который агенты могут потреблять напрямую.

Эту команду следует включать в любой верхнеуровневый SKILL.md, AGENTS.md или файл правил для агентных workflow, которые используют Coast. Это точка входа для агента, чтобы определить свой контекст выполнения.

## Output Modes

### Default (human-readable)

```bash
coast lookup
```

```text
Coast instances for worktree feature/oauth (my-app):

  dev-1  running  ★ checked out

  Primary URL:  http://dev-1.localhost:62217

  SERVICE              CANONICAL       DYNAMIC
  ★ web                3000            62217
    api                8080            63889
    postgres           5432            55681

  Examples (exec starts at the workspace root where your Coastfile is, cd to your target directory first):
    coast exec dev-1 -- sh -c "cd <dir> && <command>"
    coast logs dev-1 --service <service>
    coast ps dev-1
```

Раздел с примерами напоминает агентам (и людям), что `coast exec` стартует из корня workspace — каталога, где находится Coastfile. Чтобы выполнить команду в подкаталоге, сделайте `cd` в него внутри exec.

### Compact (`--compact`)

Возвращает JSON-массив имён экземпляров. Предназначен для скриптов и агентного инструментария, которому нужно лишь знать, на какие экземпляры нацеливаться.

```bash
coast lookup --compact
```

```text
["dev-1"]
```

Несколько экземпляров в одном worktree:

```text
["dev-1","dev-2"]
```

Нет совпадений:

```text
[]
```

### JSON (`--json`)

Возвращает полный структурированный ответ в виде красиво отформатированного JSON. Предназначен для агентов, которым нужны порты, URL и статус в машиночитаемом формате.

```bash
coast lookup --json
```

```json
{
  "project": "my-app",
  "worktree": "feature/oauth",
  "project_root": "/Users/dev/my-app",
  "instances": [
    {
      "name": "dev-1",
      "status": "Running",
      "checked_out": true,
      "branch": "feature/oauth",
      "primary_url": "http://dev-1.localhost:62217",
      "ports": [
        { "logical_name": "web", "canonical_port": 3000, "dynamic_port": 62217, "is_primary": true },
        { "logical_name": "api", "canonical_port": 8080, "dynamic_port": 63889, "is_primary": false }
      ]
    }
  ]
}
```

## How It Resolves

Lookup поднимается вверх от текущего рабочего каталога, чтобы найти ближайший Coastfile, затем определяет, в каком worktree вы находитесь:

1. Если ваш cwd находится под `{project_root}/{worktree_dir}/{name}/...`, lookup находит экземпляры, назначенные этому worktree.
2. Если ваш cwd — корень проекта (или любой каталог, не находящийся внутри worktree), lookup находит экземпляры, у которых **не назначен worktree** — те, которые всё ещё указывают на корень проекта.

Это означает, что lookup работает и из подкаталогов. Если вы находитесь в `my-app/.worktrees/feature-oauth/src/api/`, lookup всё равно определит `feature-oauth` как worktree.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Найден один или несколько подходящих экземпляров |
| 1 | Нет подходящих экземпляров (пустой результат) |

Это делает lookup пригодным для использования в условиях shell:

```bash
if coast lookup > /dev/null 2>&1; then
  coast exec dev-1 -- sh -c "cd src && npm test"
fi
```

## For Agent Workflows

Типичный паттерн интеграции агента:

1. Агент начинает работу в каталоге worktree.
2. Агент запускает `coast lookup`, чтобы узнать имена экземпляров, порты, URL и примеры команд.
3. Агент использует имя экземпляра для всех последующих команд Coast: `coast exec`, `coast logs`, `coast ps`.

```text
┌─── Agent (host machine) ────────────────────────────┐
│                                                      │
│  1. coast lookup                                     │
│       → instance names, ports, URLs, examples        │
│  2. coast exec dev-1 -- sh -c "cd src && npm test"   │
│  3. coast logs dev-1 --service web --tail 50         │
│  4. coast ps dev-1                                   │
│                                                      │
└──────────────────────────────────────────────────────┘
```

Если агент работает сразу с несколькими worktree, он запускает `coast lookup` из каталога каждого worktree, чтобы определить правильный экземпляр для каждого контекста.

См. также [Filesystem](FILESYSTEM.md) о том, как хост-агенты взаимодействуют с Coast, [Assign and Unassign](ASSIGN.md) о концепциях worktree, и [Exec & Docker](EXEC_AND_DOCKER.md) о выполнении команд внутри Coast.
