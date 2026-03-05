# パフォーマンス最適化

Coast はブランチ切り替えを高速にするよう設計されていますが、大規模なモノレポではデフォルトの挙動が不要なレイテンシを生むことがあります。このページでは、Coastfile で利用できるレバーを取り上げ、assign と unassign の時間を短縮する方法を説明します。

## Assign が遅くなる理由

`coast assign` は、Coast を新しい worktree に切り替える際にいくつかの処理を行います:

```text
coast assign dev-1 --worktree feature/payments

  1. stop affected compose services
  2. create git worktree (if new)
  3. sync gitignored files into worktree (rsync)  ← often the bottleneck
  4. remount /workspace
  5. git ls-files diff  ← can be slow in large repos
  6. restart/rebuild services
```

レイテンシの大半を占めるのは 2 つのステップです: **gitignored ファイルの同期** と **`git ls-files` diff**。どちらもリポジトリサイズに比例して増え、macOS の VirtioFS オーバーヘッドによってさらに増幅されます。

### Gitignored ファイルの同期

worktree を初めて作成する際、Coast は `rsync --link-dest` を使って、gitignored ファイル（ビルド成果物、キャッシュ、生成コード）をプロジェクトルートから新しい worktree にハードリンクします。ハードリンク自体はファイルごとにほぼ瞬時ですが、rsync は同期すべきものを見つけるために、ソースツリー内のすべてのディレクトリを走査する必要があります。

プロジェクトルートに rsync が触れるべきではない大きなディレクトリ（他の worktree、vendored 依存関係、無関係なアプリ）が含まれている場合、rsync は決してコピーしない何千ものファイルに対して無駄に潜って stat を取り、時間を浪費します。gitignored ファイルが 400,000+ あるリポジトリでは、この走査だけで 30〜60 秒かかることがあります。

Coast はこの同期から `node_modules`、`.git`、`dist`、`target`、`.worktrees`、`.coasts` など、一般的に重いディレクトリを自動的に除外します。追加のディレクトリは Coastfile の `exclude_paths` で除外できます（下記参照）。

worktree が一度同期されると `.coast-synced` マーカーが書き込まれ、以後同じ worktree への assign では同期全体がスキップされます。

### `git ls-files` Diff

assign と unassign のたびに、ブランチ間でどの tracked ファイルが変更されたかを判定するため `git ls-files` も実行されます。macOS では、ホストと Docker VM 間のすべてのファイル I/O が VirtioFS（古いセットアップでは gRPC-FUSE）をまたぎます。`git ls-files` は tracked ファイルすべてに対して stat を行うため、ファイルごとのオーバーヘッドが急速に積み上がります。tracked ファイルが 30,000 のリポジトリは、実際の diff が小さくても、5,000 のリポジトリより明らかに時間がかかります。

## `exclude_paths` — 主要なレバー

Coastfile の `exclude_paths` オプションは、**gitignored ファイルの同期**（rsync）と **`git ls-files` diff** の両方で、ディレクトリツリー全体をスキップするよう Coast に指示します。除外パス配下のファイルは worktree に存在したままですが、assign 中に走査されないだけです。

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

これは大規模モノレポにおいて最も効果の大きい最適化です。初回 assign の rsync 走査と、毎回の assign のファイル diff の両方を減らします。例えばプロジェクトに tracked ファイルが 30,000 あり、そのうち 20,000 だけが Coast で動かすサービスに関係する場合、残り 10,000 を除外すれば、各 assign の作業の 3 分の 1 を削減できます。

### 何を除外するかの選び方

目標は、Coast サービスが必要としないものをすべて除外することです。まずリポジトリ内の内容をプロファイルします:

```bash
git ls-files | cut -d'/' -f1 | sort | uniq -c | sort -rn
```

これはトップレベルディレクトリごとのファイル数を表示します。そこから、compose サービスが実際にマウントしている、または依存しているディレクトリを特定し、それ以外を除外します。

**残す（Keep）**べきディレクトリ:
- 実行中サービスにマウントされるソースコードを含む（例: アプリのディレクトリ）
- それらのサービスに import される共有ライブラリを含む
- `[assign.rebuild_triggers]` で参照されている

**除外する（Exclude）**べきディレクトリ:
- Coast で動かしていないアプリ/サービスに属する（他チームのアプリ、モバイルクライアント、CLI ツール）
- ランタイムと無関係なドキュメント、スクリプト、CI 設定、ツール類を含む
- リポジトリにチェックインされた大きな依存キャッシュ（例: vendored proto 定義、`.yarn` のオフラインキャッシュ）

### 例: 複数アプリを含むモノレポ

多くのアプリにまたがる 29,000 ファイルのモノレポだが、関係するのは 2 つだけというケース:

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

これにより diff の対象は 29,000 ファイルから約 21,000 に減り、各 assign での stat が約 28% 少なくなります。

## `[assign.services]` から非アクティブなサービスを削る

`COMPOSE_PROFILES` がサービスの一部しか起動しない場合、`[assign.services]` から非アクティブなサービスを削除してください。Coast はリストされた各サービスに対して assign 戦略を評価するため、起動していないサービスの再起動や再ビルドは無駄な作業です。

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

同じことが `[assign.rebuild_triggers]` にも当てはまります — アクティブでないサービスのエントリは削除してください。

## 可能な限り `"hot"` を使う

`"hot"` 戦略はコンテナの再起動自体をスキップします。[filesystem remount](FILESYSTEM.md) によって `/workspace` 配下のコードが差し替わり、サービス側のファイルウォッチャ（Vite、webpack、nodemon、air など）が変更を自動的に検知します。

```toml
[assign.services]
web = "hot"        # Vite/webpack dev server with HMR
api = "restart"    # Rails/Go — needs a process restart
```

`"hot"` はコンテナの stop/start サイクルを回避するため `"restart"` より高速です。ファイル監視付きの dev server を動かしているサービスには積極的に使ってください。起動時にコードを読み込み、変更を監視しないサービス（多くの Rails、Go、Java アプリ）には `"restart"` を使います。

## トリガー付きで `"rebuild"` を使う

あるサービスのデフォルト戦略が `"rebuild"` の場合、ブランチ切り替えのたびに Docker イメージを再ビルドします — たとえイメージに影響する変更が何もなくてもです。`[assign.rebuild_triggers]` を追加して、特定ファイルに変更がある場合のみリビルドするようにします:

```toml
[assign.services]
worker = "rebuild"

[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json", "package-lock.json"]
```

ブランチ間でトリガーファイルが 1 つも変わっていなければ、Coast はリビルドをスキップして、代わりに restart にフォールバックします。これにより、日常的なコード変更で高コストなイメージビルドを回避できます。

## まとめ

| 最適化 | 影響 | 対象 | 使いどころ |
|---|---|---|---|
| `exclude_paths` | 高 | rsync + git diff | Coast が必要としないディレクトリがあるリポジトリでは常に |
| 非アクティブなサービスを削除 | 中 | service restart | `COMPOSE_PROFILES` が起動サービスを絞っている場合 |
| `"hot"` 戦略 | 中 | service restart | ファイルウォッチャがあるサービス（Vite、webpack、nodemon、air） |
| `rebuild_triggers` | 中 | image rebuild | `"rebuild"` を使うサービスで、インフラ変更時だけ必要な場合 |

まず `exclude_paths` から始めてください。最小の労力で最大の効果が得られる変更です。初回 assign（rsync）と、それ以降のすべての assign（git diff）の両方を高速化します。
