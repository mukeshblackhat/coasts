# Habilidades para agentes host

Si estás usando agentes de codificación con IA (Claude Code, Codex, Conductor, Cursor o similares) en un proyecto que usa Coasts, tu agente necesita una habilidad que le enseñe cómo interactuar con el runtime de Coast. Sin esto, el agente editará archivos pero no sabrá cómo ejecutar pruebas, revisar registros ni verificar que sus cambios funcionen dentro del entorno en ejecución.

Esta guía explica cómo configurar esa habilidad.

## Por qué los agentes necesitan esto

Coasts comparte el [sistema de archivos](concepts_and_terminology/FILESYSTEM.md) entre tu máquina host y el contenedor de Coast. Tu agente edita archivos en el host y los servicios en ejecución dentro de Coast ven los cambios inmediatamente. Pero el agente aún necesita:

1. **Descubrir con qué instancia de Coast está trabajando** — `coast lookup` resuelve esto a partir del directorio actual del agente.
2. **Ejecutar comandos dentro de Coast** — las pruebas, compilaciones y otras tareas de runtime ocurren dentro del contenedor mediante `coast exec`.
3. **Leer registros y comprobar el estado del servicio** — `coast logs` y `coast ps` proporcionan al agente retroalimentación del runtime.

La habilidad a continuación le enseña al agente las tres cosas.

## La habilidad

Agrega lo siguiente a la habilidad, reglas o archivo de prompt existente de tu agente. Si tu agente ya tiene instrucciones para ejecutar pruebas o interactuar con tu entorno de desarrollo, esto debe ir junto a ellas — le enseña al agente cómo usar Coasts para operaciones de runtime.

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

## Añadir la habilidad a tu agente

La forma más rápida es dejar que el agente se configure a sí mismo. Copia el prompt de abajo en el chat de tu agente — incluye el texto de la habilidad e instrucciones para que el agente lo escriba en su propio archivo de configuración (`CLAUDE.md`, `AGENTS.md`, `.cursor/rules/coast.md`, etc.).

```prompt-copy
skills_prompt.txt
```

También puedes obtener la misma salida desde la CLI ejecutando `coast skills-prompt`.

### Configuración manual

Si prefieres agregar la habilidad tú mismo:

- **Claude Code:** Agrega el texto de la habilidad al archivo `CLAUDE.md` de tu proyecto.
- **Codex:** Agrega el texto de la habilidad al archivo `AGENTS.md` de tu proyecto.
- **Cursor:** Crea `.cursor/rules/coast.md` en la raíz de tu proyecto y pega el texto de la habilidad.
- **Otros agentes:** Pega el texto de la habilidad en cualquier archivo de prompt o reglas a nivel de proyecto que tu agente lea al iniciar.

## Lectura adicional

- Lee la [documentación de Coastfiles](coastfiles/README.md) para aprender el esquema de configuración completo
- Aprende los comandos de la [CLI de Coast](concepts_and_terminology/CLI.md) para gestionar instancias
- Explora [Coastguard](concepts_and_terminology/COASTGUARD.md), la interfaz web para observar y controlar tus Coasts
- Revisa [Conceptos y terminología](concepts_and_terminology/README.md) para obtener una visión completa de cómo funciona Coasts
