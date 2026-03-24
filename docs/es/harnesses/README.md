# Arneses

```youtube
AWhaeam9R7o
```

Cada arnés crea worktrees de git en una ubicación diferente. En Coasts, el
arreglo [`worktree_dir`](../coastfiles/WORKTREE_DIR.md) le indica dónde buscar --
incluyendo rutas externas como `~/.codex/worktrees` que requieren montajes
bind adicionales.

Cada arnés también tiene sus propias convenciones para instrucciones a nivel de proyecto, skills y comandos. La matriz a continuación muestra qué admite cada arnés para que sepas dónde poner la guía para Coasts. Cada página cubre la configuración del Coastfile, la estructura de archivos recomendada y cualquier advertencia específica de ese arnés.

Si un repositorio se usa desde varios arneses, consulta [Multiple Harnesses](MULTIPLE_HARNESSES.md).

| Harness | Worktree location | Project instructions | Skills | Commands | Page |
|---------|-------------------|----------------------|--------|----------|------|
| OpenAI Codex | `~/.codex/worktrees` | `AGENTS.md` | `.agents/skills/` | Skills surface as `/` commands | [Codex](CODEX.md) |
| Claude Code | `.claude/worktrees` | `CLAUDE.md` | `.claude/skills/` | `.claude/commands/` | [Claude Code](CLAUDE_CODE.md) |
| Cursor | `~/.cursor/worktrees/<project>` | `AGENTS.md` or `.cursor/rules/` | `.cursor/skills/` or `.agents/skills/` | `.cursor/commands/` | [Cursor](CURSOR.md) |
| Conductor | `~/conductor/workspaces/<project>` | `CLAUDE.md` | -- | -- | [Conductor](CONDUCTOR.md) |
| T3 Code | `~/.t3/worktrees/<project>` | `AGENTS.md` | `.agents/skills/` | -- | [T3 Code](T3_CODE.md) |
| Shep | `~/.shep/repos/*/wt` | `CLAUDE.md` | `.agents/skills/` or `.claude/skills/` | -- | [Shep](SHEP.md) |

## Skills vs Commands

Los skills y los comandos te permiten definir un flujo de trabajo reutilizable de `/coasts`. Puedes usar uno u ambos, según lo que admita el arnés.

Si tu arnés admite comandos y quieres un punto de entrada explícito para `/coasts`,
una opción sencilla es agregar un comando que reutilice el skill.
Los comandos se invocan explícitamente por nombre, así que sabes exactamente cuándo
se ejecuta el flujo de trabajo `/coasts`. Los skills también pueden cargarse automáticamente por el agente
según el contexto, lo cual es útil pero significa que tienes menos control sobre cuándo
se incorporan las instrucciones.

Puedes usar ambos. Si lo haces, deja que el comando reutilice el skill en lugar de
mantener una copia separada del flujo de trabajo.

Si el arnés solo admite skills (T3 Code), usa un skill. Si no admite
ninguno de los dos (Conductor), coloca el flujo de trabajo `/coasts` directamente en el archivo
de instrucciones del proyecto.
