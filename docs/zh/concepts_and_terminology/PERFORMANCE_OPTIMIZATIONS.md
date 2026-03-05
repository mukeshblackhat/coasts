# 性能优化

Coast 的设计目标是让分支切换很快，但在大型 monorepo 中，默认行为可能会引入不必要的延迟。本页介绍你可以在 Coastfile 中使用哪些调节手段来减少 assign 和 unassign 的耗时。

## 为什么 Assign 可能很慢

`coast assign` 在将一个 Coast 切换到新的 worktree 时会做几件事:

```text
coast assign dev-1 --worktree feature/payments

  1. stop affected compose services
  2. create git worktree (if new)
  3. sync gitignored files into worktree (rsync)  ← often the bottleneck
  4. remount /workspace
  5. git ls-files diff  ← can be slow in large repos
  6. restart/rebuild services
```

有两个步骤主导了延迟:**gitignored 文件同步** 和 **`git ls-files` diff**。这两者都会随仓库规模增长而变慢，并且会被 macOS VirtioFS 的开销放大。

### Gitignored 文件同步

当某个 worktree 第一次创建时，Coast 会使用 `rsync --link-dest` 将 gitignored 文件（构建产物、缓存、生成代码）从项目根目录硬链接到新的 worktree 中。对每个文件而言，硬链接几乎是瞬时的，但 rsync 仍然必须遍历源目录树中的每个目录，以发现需要同步的内容。

如果你的项目根目录包含 rsync 不应触碰的大目录——其他 worktree、被 vendored 的依赖、无关的应用——rsync 会浪费时间深入这些目录并对成千上万它永远不会复制的文件进行 stat。在一个包含 400,000+ 个 gitignored 文件的仓库中，仅仅这种遍历就可能需要 30–60 秒。

Coast 会自动在该同步中排除 `node_modules`、`.git`、`dist`、`target`、`.worktrees`、`.coasts` 以及其他常见的重型目录。额外的目录可以通过 Coastfile 中的 `exclude_paths` 进行排除（见下文）。

一旦某个 worktree 已完成同步，就会写入一个 `.coast-synced` 标记，之后对同一 worktree 的 assign 会完全跳过同步。

### `git ls-files` Diff

每次 assign 和 unassign 也都会运行 `git ls-files` 来确定分支之间哪些被跟踪的文件发生了变化。在 macOS 上，主机与 Docker VM 之间的所有文件 I/O 都要经过 VirtioFS（或较旧环境中的 gRPC-FUSE）。`git ls-files` 操作会对每个被跟踪文件进行 stat，而单文件的开销会很快叠加。一个有 30,000 个被跟踪文件的仓库会明显比只有 5,000 个的仓库耗时更长，即使实际 diff 很小。

## `exclude_paths` — 主要杠杆

Coastfile 中的 `exclude_paths` 选项会让 Coast 在 **gitignored 文件同步**（rsync）和 **`git ls-files` diff** 两个阶段都跳过整个目录树。被排除路径下的文件仍然存在于 worktree 中——只是 assign 期间不会遍历它们。

```toml
[assign]
default = "none"
exclude_paths = [
    "docs",
    "scripts",
    "test-fixtures",
    "apps/mobile",
]
```

对于大型 monorepo，这是影响最大的单项优化。它既减少首次 assign 时的 rsync 遍历，也减少每次 assign 时的文件 diff。如果你的项目有 30,000 个被跟踪文件，但只有 20,000 个与 Coast 中运行的服务相关，那么把另外 10,000 个排除掉，就能让每次 assign 的工作量减少三分之一。

### 如何选择要排除的内容

目标是排除所有你的 Coast 服务不需要的东西。先从分析仓库内容开始:

```bash
git ls-files | cut -d'/' -f1 | sort | uniq -c | sort -rn
```

这会显示每个顶层目录的文件数量。然后识别哪些目录是你的 compose 服务实际会挂载或依赖的，并排除其余部分。

**保留**以下目录:
- 包含挂载到运行中服务的源代码（例如你的应用目录）
- 包含这些服务会导入的共享库
- 在 `[assign.rebuild_triggers]` 中被引用

**排除**以下目录:
- 属于不在你的 Coast 中运行的应用或服务（其他团队的应用、移动端客户端、CLI 工具）
- 包含与运行时无关的文档、脚本、CI 配置或工具
- 仓库中提交的体积很大的依赖缓存（例如 vendored 的 proto 定义、`.yarn` 离线缓存）

### 示例:包含多个应用的 Monorepo

一个 monorepo 有 29,000 个文件分布在许多应用中，但只有两个相关:

```text
  13,000  bookface/         ← active
   7,000  ycinternal/       ← active
     850  shared/           ← used by both
   3,800  .yarn/            ← excludable
   2,500  startupschool/    ← excludable
     500  misc/             ← excludable
     300  ycapp/            ← excludable
     ...  (12 more dirs)    ← excludable
```

```toml
[assign]
default = "none"
exclude_paths = [
    ".yarn",
    "startupschool",
    "misc",
    "ycapp",
    "apply",
    "cli",
    "deploy",
    "lambdas",
    # ... any other directories not needed by active services
]
```

这将 diff 范围从 29,000 个文件减少到约 21,000 个——也就是每次 assign 需要进行的 stat 大约减少 28%。

## 从 `[assign.services]` 中移除不活跃的服务

如果你的 `COMPOSE_PROFILES` 只启动一部分服务，就把不活跃的服务从 `[assign.services]` 中移除。Coast 会对每个列出的服务评估 assign 策略，而重启或重建一个并未运行的服务就是浪费工作。

```toml
# Bad — restarts services that aren't running
[assign.services]
web = "restart"
api = "restart"
mobile-api = "restart"   # not in COMPOSE_PROFILES
batch-worker = "restart"  # not in COMPOSE_PROFILES

# Good — only services that are actually running
[assign.services]
web = "restart"
api = "restart"
```

同样也适用于 `[assign.rebuild_triggers]` ——移除那些不活跃服务的条目。

## 尽可能使用 `"hot"`

`"hot"` 策略会完全跳过容器重启。[filesystem remount](FILESYSTEM.md) 会替换 `/workspace` 下的代码，而服务的文件监视器（Vite、webpack、nodemon、air 等）会自动捕获变更。

```toml
[assign.services]
web = "hot"        # Vite/webpack dev server with HMR
api = "restart"    # Rails/Go — needs a process restart
```

`"hot"` 比 `"restart"` 更快，因为它避免了容器 stop/start 周期。对任何运行带文件监视的开发服务器的服务都应使用它。把 `"restart"` 留给那些仅在启动时加载代码且不会监视变更的服务（大多数 Rails、Go 和 Java 应用）。

## 配合触发器使用 `"rebuild"`

如果某个服务的默认策略是 `"rebuild"`，每次分支切换都会重建 Docker 镜像——即使没有任何影响镜像的内容发生变化。添加 `[assign.rebuild_triggers]` 来把重建限制在特定文件发生变化时才触发:

```toml
[assign.services]
worker = "rebuild"

[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json", "package-lock.json"]
```

如果分支之间没有任何触发文件发生变化，Coast 会跳过重建并改为回退到 restart。这能避免在日常代码变更时进行昂贵的镜像构建。

## 总结

| 优化 | 影响 | 影响对象 | 何时使用 |
|---|---|---|---|
| `exclude_paths` | 高 | rsync + git diff | 始终使用，在任何包含 Coast 不需要目录的仓库中 |
| 移除不活跃服务 | 中 | service restart | 当 `COMPOSE_PROFILES` 限制实际运行的服务时 |
| `"hot"` 策略 | 中 | service restart | 具有文件监视器的服务（Vite、webpack、nodemon、air） |
| `rebuild_triggers` | 中 | image rebuild | 使用 `"rebuild"` 且只有在基础设施变更时才需要重建的服务 |

从 `exclude_paths` 开始。它是你能做的最低成本、最高收益的改动。它能同时加速首次 assign（rsync）以及之后每一次 assign（git diff）。
