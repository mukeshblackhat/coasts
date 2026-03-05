# Lookup

`coast lookup` は、呼び出し元の現在の作業ディレクトリに対して、どの Coast インスタンスが稼働しているかを検出します。これはホスト側エージェントが状況把握のために最初に実行すべきコマンドです — 「ここでコードを編集しているが、どの Coast とやり取りすべきか？」

```bash
coast lookup
```

Lookup は、あなたが [worktree](ASSIGN.md) の内側にいるのか、あるいはプロジェクトルートにいるのかを検出し、該当するインスタンスをデーモンに問い合わせ、ポート、URL、サンプルコマンド付きで結果を表示します。

## Why This Exists

ホスト上で動作する AI コーディングエージェント（Cursor、Claude Code、Codex など）は、[shared filesystem](FILESYSTEM.md) を通じてファイルを編集し、実行時の操作のために Coast CLI コマンドを呼び出します。しかしその前に、エージェントは基本的な質問に答える必要があります: **自分が作業しているディレクトリに対応する Coast インスタンスはどれか？**

`coast lookup` がない場合、エージェントは `coast ls` を実行し、インスタンス表全体を解析し、どの worktree にいるのかを突き止め、照合しなければなりません。`coast lookup` はそれらを 1 ステップで行い、エージェントが直接消費できる構造化出力を返します。

このコマンドは、Coast を使うエージェントワークフロー向けのトップレベル SKILL.md、AGENTS.md、またはルールファイルに含めるべきです。これは、エージェントが実行時コンテキストを発見するための入口です。

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

examples セクションは、`coast exec` がワークスペースルート — Coastfile が存在するディレクトリ — から開始されることを、エージェント（および人間）に思い出させます。サブディレクトリでコマンドを実行するには、exec 内でそのディレクトリへ `cd` します。

### Compact (`--compact`)

インスタンス名の JSON 配列を返します。どのインスタンスを対象にすべきかだけを知りたいスクリプトやエージェントツール向けです。

```bash
coast lookup --compact
```

```text
["dev-1"]
```

同じ worktree 上に複数のインスタンスがある場合:

```text
["dev-1","dev-2"]
```

一致なし:

```text
[]
```

### JSON (`--json`)

完全な構造化レスポンスを整形済み JSON として返します。ポート、URL、ステータスを機械可読形式で必要とするエージェント向けです。

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

Lookup は、現在の作業ディレクトリから上位へ辿って最も近い Coastfile を見つけ、その後、あなたがどの worktree にいるのかを判定します:

1. cwd が `{project_root}/{worktree_dir}/{name}/...` の配下にある場合、lookup はその worktree に割り当てられたインスタンスを見つけます。
2. cwd がプロジェクトルート（または worktree の内側ではない任意のディレクトリ）の場合、lookup は **worktree が割り当てられていない** インスタンス — まだプロジェクトルートを指しているもの — を見つけます。

これは、lookup がサブディレクトリからでも機能することを意味します。`my-app/.worktrees/feature-oauth/src/api/` にいる場合でも、lookup は `feature-oauth` を worktree として解決します。

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | 1 つ以上の一致するインスタンスが見つかった |
| 1 | 一致するインスタンスがない（空の結果） |

これにより、lookup をシェルの条件分岐で利用できます:

```bash
if coast lookup > /dev/null 2>&1; then
  coast exec dev-1 -- sh -c "cd src && npm test"
fi
```

## For Agent Workflows

典型的なエージェント統合パターン:

1. エージェントは worktree ディレクトリで作業を開始する。
2. エージェントは `coast lookup` を実行して、インスタンス名、ポート、URL、サンプルコマンドを発見する。
3. エージェントは以後のすべての Coast コマンドでインスタンス名を使用する: `coast exec`、`coast logs`、`coast ps`。

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

エージェントが複数の worktree にまたがって作業している場合、各 worktree ディレクトリから `coast lookup` を実行して、それぞれのコンテキストに対する正しいインスタンスを解決します。

ホストエージェントが Coast とどのようにやり取りするかについては [Filesystem](FILESYSTEM.md) を、worktree の概念については [Assign and Unassign](ASSIGN.md) を、Coast 内でコマンドを実行する方法については [Exec & Docker](EXEC_AND_DOCKER.md) を参照してください。
