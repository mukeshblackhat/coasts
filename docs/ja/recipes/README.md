# レシピ

レシピは、一般的なプロジェクト構成向けの、完全で注釈付きの Coastfile サンプルです。各レシピには、コピーして適用できる完全な Coastfile と、各判断がなぜ行われたのかを説明するセクションごとのウォークスルーが含まれます。

Coastfile が初めての場合は、まず [Coastfiles reference](../coastfiles/README.md) を参照してください。レシピは中核となる概念に精通していることを前提としています。

- [Full-Stack Monorepo](FULLSTACK_MONOREPO.md) — ホスト上で共有する Postgres と Redis、bare-service の Vite フロントエンド、そして compose による dockerized バックエンド。ボリューム戦略、healthchecks、assign のチューニング、大規模リポジトリ向けの `exclude_paths` をカバーします。
