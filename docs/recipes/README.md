# Recipes

Recipes are complete, annotated Coastfile examples for common project shapes. Each recipe includes a full Coastfile you can copy and adapt, followed by a section-by-section walkthrough explaining why each decision was made.

If you are new to Coastfiles, start with the [Coastfiles reference](../coastfiles/README.md) first. Recipes assume familiarity with the core concepts.

- [Full-Stack Monorepo](FULLSTACK_MONOREPO.md) — shared Postgres and Redis on the host, bare-service Vite frontends, and a dockerized backend via compose. Covers volume strategies, healthchecks, assign tuning, and `exclude_paths` for large repos.
