# ホストエージェント向けスキル

Coasts を使用するプロジェクトで AI コーディングエージェント（Claude Code、Codex、Conductor、Cursor など）を使っている場合、エージェントには Coast ランタイムとのやり取り方法を教えるスキルが必要です。これがないと、エージェントはファイルを編集できても、テストの実行方法、ログの確認方法、あるいは実行中の環境内で変更が正しく動作するかを検証する方法を理解できません。

このガイドでは、そのスキルを設定する方法を説明します。

## エージェントにこれが必要な理由

Coasts は、ホストマシンと Coast コンテナの間で [filesystem](concepts_and_terminology/FILESYSTEM.md) を共有します。エージェントはホスト上でファイルを編集し、Coast 内で実行中のサービスはその変更を即座に認識します。しかし、エージェントは依然として次のことを行う必要があります。

1. **どの Coast インスタンスで作業しているかを特定する** — `coast lookup` は、エージェントの現在のディレクトリからこれを解決します。
2. **Coast 内でコマンドを実行する** — テスト、ビルド、その他のランタイムタスクは、`coast exec` を介してコンテナ内で実行されます。
3. **ログを読み、サービスの状態を確認する** — `coast logs` と `coast ps` は、エージェントにランタイムのフィードバックを提供します。

以下のスキルは、この 3 つすべてをエージェントに教えます。

## スキル

以下を、エージェントの既存のスキル、ルール、またはプロンプトファイルに追加してください。エージェントにすでにテスト実行や開発環境とのやり取りに関する指示がある場合、これはそれらと並べて置くべきものです。これは、ランタイム操作のために Coasts を使用する方法をエージェントに教えます。

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

## スキルをエージェントに追加する

最も手早い方法は、エージェント自身にセットアップさせることです。以下のプロンプトをエージェントのチャットにコピーしてください。これにはスキルテキストと、それを自身の設定ファイル（`CLAUDE.md`、`AGENTS.md`、`.cursor/rules/coast.md` など）に書き込むための指示が含まれています。

```prompt-copy
skills_prompt.txt
```

CLI から `coast skills-prompt` を実行しても、同じ出力を取得できます。

### 手動セットアップ

自分でスキルを追加したい場合:

- **Claude Code:** スキルテキストをプロジェクトの `CLAUDE.md` ファイルに追加してください。
- **Codex:** スキルテキストをプロジェクトの `AGENTS.md` ファイルに追加してください。
- **Cursor:** プロジェクトルートに `.cursor/rules/coast.md` を作成し、スキルテキストを貼り付けてください。
- **Other agents:** エージェントが起動時に読み込むプロジェクトレベルのプロンプトまたはルールファイルに、スキルテキストを貼り付けてください。

## さらに読む

- 完全な設定スキーマを学ぶには、[Coastfiles documentation](coastfiles/README.md) を読んでください
- インスタンス管理用コマンドについては、[Coast CLI](concepts_and_terminology/CLI.md) を参照してください
- Coasts を観察および制御するための Web UI である [Coastguard](concepts_and_terminology/COASTGUARD.md) を確認してください
- Coasts の仕組み全体を把握するには、[Concepts & Terminology](concepts_and_terminology/README.md) を参照してください
