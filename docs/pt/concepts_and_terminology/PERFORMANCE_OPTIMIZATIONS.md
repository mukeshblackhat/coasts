# Otimizações de Desempenho

O Coast foi projetado para tornar a troca de branches rápida, mas em monorepos grandes o comportamento padrão pode introduzir latência desnecessária. Esta página cobre as alavancas disponíveis no seu Coastfile para reduzir os tempos de assign e unassign.

## Por que Assign Pode Ser Lento

`coast assign` faz várias coisas ao alternar um Coast para um novo worktree:

```text
coast assign dev-1 --worktree feature/payments

  1. stop affected compose services
  2. create git worktree (if new)
  3. sync gitignored files into worktree (rsync)  ← often the bottleneck
  4. remount /workspace
  5. git ls-files diff  ← can be slow in large repos
  6. restart/rebuild services
```

Dois passos dominam a latência: a **sincronização de arquivos ignorados pelo git** e o **diff do `git ls-files`**. Ambos escalam com o tamanho do repositório e são amplificados pelo overhead do VirtioFS no macOS.

### Sincronização de Arquivos Ignorados pelo Git

Quando um worktree é criado pela primeira vez, o Coast usa `rsync --link-dest` para criar hardlinks de arquivos ignorados pelo git (artefatos de build, caches, código gerado) da raiz do projeto para o novo worktree. Hardlinks são quase instantâneos por arquivo, mas o rsync ainda precisa percorrer cada diretório na árvore de origem para descobrir o que precisa ser sincronizado.

Se a raiz do seu projeto contiver diretórios grandes que o rsync não deveria tocar — outros worktrees, dependências vendorizadas, apps não relacionadas — o rsync perde tempo descendo e fazendo stat em milhares de arquivos que ele nunca irá copiar. Em um repo com 400.000+ arquivos ignorados pelo git, somente essa travessia pode levar 30–60 segundos.

O Coast exclui automaticamente `node_modules`, `.git`, `dist`, `target`, `.worktrees`, `.coasts` e outros diretórios pesados comuns dessa sincronização. Diretórios adicionais podem ser excluídos via `exclude_paths` no seu Coastfile (veja abaixo).

Uma vez que um worktree tenha sido sincronizado, um marcador `.coast-synced` é gravado e assigns subsequentes para o mesmo worktree pulam a sincronização por completo.

### Diff do `git ls-files`

Cada assign e unassign também executa `git ls-files` para determinar quais arquivos versionados mudaram entre branches. No macOS, todo I/O de arquivos entre o host e a VM do Docker atravessa o VirtioFS (ou gRPC-FUSE em configurações mais antigas). A operação `git ls-files` faz stat em cada arquivo versionado, e o overhead por arquivo se acumula rapidamente. Um repo com 30.000 arquivos versionados levará perceptivelmente mais tempo do que um com 5.000, mesmo que o diff real seja pequeno.

## `exclude_paths` — A Principal Alavanca

A opção `exclude_paths` no seu Coastfile diz ao Coast para pular árvores de diretórios inteiras durante tanto a **sincronização de arquivos ignorados pelo git** (rsync) quanto o **diff do `git ls-files`**. Arquivos sob caminhos excluídos ainda estão presentes no worktree — eles apenas não são percorridos durante o assign.

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

Esta é a otimização única mais impactante para monorepos grandes. Ela reduz tanto a travessia do rsync no primeiro assign quanto o diff de arquivos em cada assign. Se seu projeto tem 30.000 arquivos versionados mas apenas 20.000 são relevantes para os serviços rodando no Coast, excluir os outros 10.000 corta um terço do trabalho de cada assign.

### Escolhendo o que Excluir

O objetivo é excluir tudo o que seus serviços do Coast não precisam. Comece perfilando o que há no seu repo:

```bash
git ls-files | cut -d'/' -f1 | sort | uniq -c | sort -rn
```

Isso mostra a contagem de arquivos por diretório de nível superior. A partir daí, identifique quais diretórios seus serviços do compose realmente montam ou dos quais dependem, e exclua o resto.

**Mantenha** diretórios que:
- Contêm código-fonte montado em serviços em execução (por exemplo, seus diretórios de app)
- Contêm bibliotecas compartilhadas importadas por esses serviços
- São referenciados em `[assign.rebuild_triggers]`

**Exclua** diretórios que:
- Pertencem a apps ou serviços que não estão rodando no seu Coast (apps de outras equipes, clientes mobile, ferramentas CLI)
- Contêm documentação, scripts, configs de CI ou tooling não relacionado ao runtime
- São grandes caches de dependências commitados no repo (por exemplo, definições de proto vendorizadas, cache offline do `.yarn`)

### Exemplo: Monorepo com Vários Apps

Um monorepo com 29.000 arquivos em muitos apps, mas apenas dois são relevantes:

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

Isso reduz a superfície do diff de 29.000 arquivos para ~21.000 — aproximadamente 28% menos stats em cada assign.

## Remova Serviços Inativos de `[assign.services]`

Se o seu `COMPOSE_PROFILES` só inicia um subconjunto de serviços, remova os serviços inativos de `[assign.services]`. O Coast avalia a estratégia de assign para cada serviço listado, e reiniciar ou rebuildar um serviço que não está rodando é trabalho desperdiçado.

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

O mesmo se aplica a `[assign.rebuild_triggers]` — remova entradas para serviços que não estão ativos.

## Use `"hot"` Sempre que Possível

A estratégia `"hot"` pula completamente o restart do container. O [remount do filesystem](FILESYSTEM.md) troca o código sob `/workspace` e o file watcher do serviço (Vite, webpack, nodemon, air, etc.) detecta as mudanças automaticamente.

```toml
[assign.services]
web = "hot"        # Vite/webpack dev server with HMR
api = "restart"    # Rails/Go — needs a process restart
```

`"hot"` é mais rápido que `"restart"` porque evita o ciclo de parar/iniciar o container. Use-o para qualquer serviço que rode um servidor de desenvolvimento com file watching. Reserve `"restart"` para serviços que carregam código na inicialização e não observam mudanças (a maioria dos apps Rails, Go e Java).

## Use `"rebuild"` com Triggers

Se a estratégia padrão de um serviço é `"rebuild"`, toda troca de branch faz rebuild da imagem Docker — mesmo que nada que afete a imagem tenha mudado. Adicione `[assign.rebuild_triggers]` para condicionar o rebuild a arquivos específicos:

```toml
[assign.services]
worker = "rebuild"

[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json", "package-lock.json"]
```

Se nenhum dos arquivos de trigger mudou entre branches, o Coast pula o rebuild e, em vez disso, faz fallback para um restart. Isso evita builds de imagem caros em mudanças rotineiras de código.

## Resumo

| Otimização | Impacto | Afeta | Quando usar |
|---|---|---|---|
| `exclude_paths` | Alto | rsync + git diff | Sempre, em qualquer repo com diretórios de que seu Coast não precisa |
| Remover serviços inativos | Médio | restart de serviço | Quando `COMPOSE_PROFILES` limita quais serviços rodam |
| Estratégia `"hot"` | Médio | restart de serviço | Serviços com file watchers (Vite, webpack, nodemon, air) |
| `rebuild_triggers` | Médio | rebuild de imagem | Serviços usando `"rebuild"` que só precisam disso para mudanças de infra |

Comece com `exclude_paths`. É a mudança de menor esforço e maior impacto que você pode fazer. Ela acelera tanto o primeiro assign (rsync) quanto cada assign subsequente (git diff).
