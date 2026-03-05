# 割り当てと割り当て解除

割り当て（assign）と割り当て解除（unassign）は、Coast インスタンスがどの worktree を指すかを制御します。マウントレベルでの worktree 切り替えの仕組みについては [Filesystem](FILESYSTEM.md) を参照してください。

## Assign

`coast assign` は Coast インスタンスを特定の worktree に切り替えます。Coast は worktree がまだ存在しない場合は作成し、Coast 内のコードを更新し、設定された割り当て戦略に従ってサービスを再起動します。

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

割り当て後、`dev-1` は `feature/oauth` ブランチで動作し、すべてのサービスが起動した状態になります。

## Unassign

`coast unassign` は Coast インスタンスをプロジェクトルート（main/master ブランチ）に戻します。worktree の関連付けが解除され、Coast はプライマリリポジトリから実行する状態に戻ります。

```text
coast unassign dev-1

┌─── dev-1 ──────────────────┐
│  branch: main              │
│  worktree: -               │
└────────────────────────────┘
```

## Assign Strategies

Coast が新しい worktree に割り当てられると、各サービスはコード変更への対処方法を知る必要があります。これは [Coastfile](COASTFILE_TYPES.md) の `[assign]` 配下でサービスごとに設定します。

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

利用可能な戦略は次のとおりです。

- **none** — 何もしません。Postgres や Redis のようにブランチ間で変化しないサービスに使用します。
- **hot** — ファイルシステムのみを入れ替えます。サービスは稼働し続け、マウント伝播とファイルウォッチャ（例: ホットリロード付きの開発サーバ）によって変更を取り込みます。
- **restart** — サービスコンテナを再起動します。プロセス再起動だけが必要なインタプリタ言語のサービスに使用します。これがデフォルトです。
- **rebuild** — サービスイメージを再ビルドして再起動します。ブランチ変更が `Dockerfile` やビルド時依存関係に影響する場合に使用します。

また、再ビルドトリガーを指定して、特定のファイルが変更されたときにのみサービスが再ビルドされるようにすることもできます。

```toml
[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json"]
```

ブランチ間でトリガーファイルが1つも変更されていない場合、戦略が `rebuild` に設定されていてもサービスは再ビルドをスキップします。

## Deleted Worktrees

割り当てられた worktree が削除された場合、`coastd` デーモンはそのインスタンスを自動的に割り当て解除し、メインの Git リポジトリルートに戻します。

---

> **Tip: 大規模コードベースでの割り当て遅延を減らす**
>
> 内部的に、Coast は worktree のマウントまたはアンマウントのたびに `git ls-files` を実行します。大規模なコードベースや多数のファイルを含むリポジトリでは、これにより assign と unassign の操作に目立つ遅延が追加されることがあります。
>
> Coastfile の `exclude_paths` を使用して、実行中のサービスに関係のないディレクトリをスキップしてください。完全なガイドは [Performance Optimizations](PERFORMANCE_OPTIMIZATIONS.md) を参照してください。
