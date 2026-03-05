# Назначение и снятие назначения

Назначение и снятие назначения управляют тем, на какое рабочее дерево (worktree) указывает экземпляр Coast. См. [Filesystem](FILESYSTEM.md), чтобы узнать, как переключение worktree работает на уровне монтирования.

## Назначение

`coast assign` переключает экземпляр Coast на конкретное worktree. Coast создаёт worktree, если оно ещё не существует, обновляет код внутри Coast и перезапускает сервисы согласно настроенной стратегии назначения.

```bash
coast assign dev-1 --worktree feature/oauth
```

```text
Before:
┌─── dev-1 ──────────────────┐
│  branch: main              │
│  worktree: -               │
└────────────────────────────┘

coast assign dev-1 --worktree feature/oauth

After:
┌─── dev-1 ──────────────────┐
│  branch: feature/oauth     │
│  worktree: feature/oauth   │
│                            │
│  postgres → skipped (none) │
│  web      → hot swapped    │
│  api      → restarted      │
│  worker   → rebuilt        │
└────────────────────────────┘
```

После назначения `dev-1` работает на ветке `feature/oauth` со всеми запущенными сервисами.

## Снятие назначения

`coast unassign` переключает экземпляр Coast обратно на корень проекта (ваша ветка main/master). Привязка к worktree удаляется, и Coast возвращается к запуску из основного репозитория.

```text
coast unassign dev-1

┌─── dev-1 ──────────────────┐
│  branch: main              │
│  worktree: -               │
└────────────────────────────┘
```

## Стратегии назначения

Когда Coast назначается на новое worktree, каждому сервису нужно знать, как обработать изменение кода. Это настраивается для каждого сервиса в вашем [Coastfile](COASTFILE_TYPES.md) в разделе `[assign]`:

```toml
[assign]
default = "restart"

[assign.services]
postgres = "none"
redis = "none"
web = "hot"
worker = "rebuild"
```

```text
coast assign dev-1 --worktree feature/billing

  postgres (strategy: none)    →  skipped, unchanged between branches
  redis (strategy: none)       →  skipped, unchanged between branches
  web (strategy: hot)          →  filesystem swapped, file watcher picks it up
  api (strategy: restart)      →  container restarted
  worker (strategy: rebuild)   →  image rebuilt, container restarted
```

Доступные стратегии:

- **none** — ничего не делать. Используйте для сервисов, которые не меняются между ветками, например Postgres или Redis.
- **hot** — заменить только файловую систему. Сервис продолжает работать и подхватывает изменения через распространение монтирования и файловые наблюдатели (например, dev-сервер с hot reload).
- **restart** — перезапустить контейнер сервиса. Используйте для интерпретируемых сервисов, которым нужен лишь перезапуск процесса. Это значение по умолчанию.
- **rebuild** — пересобрать образ сервиса и перезапустить. Используйте, когда смена ветки влияет на `Dockerfile` или зависимости времени сборки.

Вы также можете указать триггеры пересборки, чтобы сервис пересобирался только при изменении конкретных файлов:

```toml
[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json"]
```

Если ни один из файлов-триггеров не изменился между ветками, сервис пропускает пересборку, даже если стратегия установлена в `rebuild`.

## Удалённые рабочие деревья

Если назначенное worktree удалено, демон `coastd` автоматически снимает назначение этого экземпляра, возвращая его к корню основного Git-репозитория.

---

> **Совет: Снижение задержки при назначении в больших кодовых базах**
>
> Внутри Coast запускает `git ls-files` всякий раз, когда worktree монтируется или размонтируется. В больших кодовых базах или репозиториях с большим количеством файлов это может добавлять заметную задержку операциям назначения и снятия назначения.
>
> Используйте `exclude_paths` в вашем Coastfile, чтобы пропускать директории, не относящиеся к вашим запущенным сервисам. Полное руководство см. в [Performance Optimizations](PERFORMANCE_OPTIMIZATIONS.md).
