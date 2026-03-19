# Habilidades para Agentes Host

Se você estiver usando agentes de programação com IA (Claude Code, Codex, Conductor, Cursor ou similares) em um projeto que usa Coasts, seu agente precisa de uma habilidade que o ensine a interagir com o runtime do Coast. Sem isso, o agente editará arquivos, mas não saberá como executar testes, verificar logs ou confirmar que suas mudanças funcionam dentro do ambiente em execução.

Este guia mostra como configurar essa habilidade.

## Por que os Agentes Precisam Disso

Os Coasts compartilham o [sistema de arquivos](concepts_and_terminology/FILESYSTEM.md) entre sua máquina host e o contêiner do Coast. Seu agente edita arquivos no host e os serviços em execução dentro do Coast veem as mudanças imediatamente. Mas o agente ainda precisa:

1. **Descobrir com qual instância do Coast está trabalhando** — `coast lookup` resolve isso a partir do diretório atual do agente.
2. **Executar comandos dentro do Coast** — testes, builds e outras tarefas de runtime acontecem dentro do contêiner via `coast exec`.
3. **Ler logs e verificar o status dos serviços** — `coast logs` e `coast ps` dão ao agente feedback de runtime.

A habilidade abaixo ensina ao agente os três.

## A Habilidade

Adicione o seguinte à habilidade, regras ou arquivo de prompt existente do seu agente. Se o seu agente já tiver instruções para executar testes ou interagir com seu ambiente de desenvolvimento, isso deve ficar junto delas — isso ensina o agente a usar Coasts para operações de runtime.

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

## Adicionando a Habilidade ao Seu Agente

A maneira mais rápida é deixar que o próprio agente se configure. Copie o prompt abaixo para o chat do seu agente — ele inclui o texto da habilidade e instruções para o agente gravá-lo em seu próprio arquivo de configuração (`CLAUDE.md`, `AGENTS.md`, `.cursor/rules/coast.md`, etc.).

```prompt-copy
skills_prompt.txt
```

Você também pode obter a mesma saída pela CLI executando `coast skills-prompt`.

### Configuração manual

Se preferir adicionar a habilidade por conta própria:

- **Claude Code:** Adicione o texto da habilidade ao arquivo `CLAUDE.md` do seu projeto.
- **Codex:** Adicione o texto da habilidade ao arquivo `AGENTS.md` do seu projeto.
- **Cursor:** Crie `.cursor/rules/coast.md` na raiz do seu projeto e cole o texto da habilidade.
- **Outros agentes:** Cole o texto da habilidade em qualquer arquivo de prompt ou regras no nível do projeto que seu agente leia na inicialização.

## Leitura adicional

- Leia a [documentação dos Coastfiles](coastfiles/README.md) para aprender o esquema completo de configuração
- Aprenda os comandos da [CLI do Coast](concepts_and_terminology/CLI.md) para gerenciar instâncias
- Explore o [Coastguard](concepts_and_terminology/COASTGUARD.md), a interface web para observar e controlar seus Coasts
- Navegue por [Conceitos e Terminologia](concepts_and_terminology/README.md) para ter a visão completa de como os Coasts funcionam
