# Optimizaciones de Rendimiento

Coast está diseñado para que el cambio de ramas sea rápido, pero en monorepos grandes el comportamiento predeterminado puede introducir latencia innecesaria. Esta página cubre las palancas disponibles en tu Coastfile para reducir los tiempos de asignación y desasignación.

## Por Qué Assign Puede Ser Lento

`coast assign` hace varias cosas al cambiar un Coast a un nuevo worktree:

```text
coast assign dev-1 --worktree feature/payments

  1. stop affected compose services
  2. create git worktree (if new)
  3. sync gitignored files into worktree (rsync)  ← often the bottleneck
  4. remount /workspace
  5. git ls-files diff  ← can be slow in large repos
  6. restart/rebuild services
```

Dos pasos dominan la latencia: la **sincronización de archivos ignorados por git** y el **diff de `git ls-files`**. Ambos escalan con el tamaño del repositorio y se amplifican por la sobrecarga de VirtioFS en macOS.

### Sincronización de Archivos Ignorados por Git

Cuando un worktree se crea por primera vez, Coast usa `rsync --link-dest` para crear hardlinks de los archivos ignorados por git (artefactos de build, cachés, código generado) desde la raíz del proyecto hacia el nuevo worktree. Los hardlinks son casi instantáneos por archivo, pero rsync aun así debe recorrer cada directorio en el árbol de origen para descubrir qué necesita sincronizarse.

Si la raíz de tu proyecto contiene directorios grandes que rsync no debería tocar — otros worktrees, dependencias vendorizadas, aplicaciones no relacionadas — rsync pierde tiempo descendiendo y haciendo stat a miles de archivos que nunca copiará. En un repo con 400,000+ archivos ignorados por git, solo este recorrido puede tardar 30–60 segundos.

Coast excluye automáticamente `node_modules`, `.git`, `dist`, `target`, `.worktrees`, `.coasts` y otros directorios pesados comunes de esta sincronización. Se pueden excluir directorios adicionales mediante `exclude_paths` en tu Coastfile (ver abajo).

Una vez que un worktree ha sido sincronizado, se escribe un marcador `.coast-synced` y las asignaciones posteriores al mismo worktree omiten por completo la sincronización.

### Diff de `git ls-files`

Cada assign y unassign también ejecuta `git ls-files` para determinar qué archivos rastreados cambiaron entre ramas. En macOS, toda E/S de archivos entre el host y la VM de Docker cruza VirtioFS (o gRPC-FUSE en configuraciones más antiguas). La operación `git ls-files` hace stat de cada archivo rastreado, y la sobrecarga por archivo se acumula rápidamente. Un repo con 30,000 archivos rastreados tardará notablemente más que uno con 5,000, incluso si el diff real es pequeño.

## `exclude_paths` — La Palanca Principal

La opción `exclude_paths` en tu Coastfile le indica a Coast que omita árboles de directorios completos durante tanto la **sincronización de archivos ignorados por git** (rsync) como el **diff de `git ls-files`**. Los archivos bajo rutas excluidas siguen presentes en el worktree — simplemente no se recorren durante assign.

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

Esta es la optimización más impactante para monorepos grandes. Reduce tanto el recorrido de rsync en el primer assign como el diff de archivos en cada assign. Si tu proyecto tiene 30,000 archivos rastreados pero solo 20,000 son relevantes para los servicios que se ejecutan en el Coast, excluir los otros 10,000 reduce un tercio del trabajo en cada assign.

### Elegir Qué Excluir

El objetivo es excluir todo lo que tus servicios de Coast no necesitan. Empieza perfilando qué hay en tu repo:

```bash
git ls-files | cut -d'/' -f1 | sort | uniq -c | sort -rn
```

Esto muestra el conteo de archivos por directorio de nivel superior. A partir de ahí, identifica qué directorios tus servicios de compose realmente montan o de los que dependen, y excluye el resto.

**Mantén** directorios que:
- Contienen código fuente montado en servicios en ejecución (p. ej., tus directorios de la app)
- Contienen librerías compartidas importadas por esos servicios
- Están referenciados en `[assign.rebuild_triggers]`

**Excluye** directorios que:
- Pertenecen a apps o servicios que no se ejecutan en tu Coast (apps de otros equipos, clientes móviles, herramientas CLI)
- Contienen documentación, scripts, configuraciones de CI o tooling no relacionado con el runtime
- Son cachés grandes de dependencias incluidos en el repo (p. ej., definiciones de proto vendorizadas, caché offline de `.yarn`)

### Ejemplo: Monorepo Con Múltiples Apps

Un monorepo con 29,000 archivos repartidos entre muchas apps, pero solo dos son relevantes:

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

Esto reduce la superficie del diff de 29,000 archivos a ~21,000 — aproximadamente 28% menos stats en cada assign.

## Recorta Servicios Inactivos de `[assign.services]`

Si tu `COMPOSE_PROFILES` solo inicia un subconjunto de servicios, elimina los servicios inactivos de `[assign.services]`. Coast evalúa la estrategia de assign para cada servicio listado, y reiniciar o reconstruir un servicio que no está ejecutándose es trabajo desperdiciado.

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

Lo mismo aplica a `[assign.rebuild_triggers]` — elimina entradas de servicios que no estén activos.

## Usa `"hot"` Cuando Sea Posible

La estrategia `"hot"` omite por completo el reinicio del contenedor. El [remount del filesystem](FILESYSTEM.md) intercambia el código bajo `/workspace` y el file watcher del servicio (Vite, webpack, nodemon, air, etc.) detecta los cambios automáticamente.

```toml
[assign.services]
web = "hot"        # Vite/webpack dev server with HMR
api = "restart"    # Rails/Go — needs a process restart
```

`"hot"` es más rápido que `"restart"` porque evita el ciclo de stop/start del contenedor. Úsalo para cualquier servicio que ejecute un servidor de desarrollo con observación de archivos. Reserva `"restart"` para servicios que cargan el código al inicio y no observan cambios (la mayoría de apps Rails, Go y Java).

## Usa `"rebuild"` Con Triggers

Si la estrategia predeterminada de un servicio es `"rebuild"`, cada cambio de rama reconstruye la imagen de Docker — incluso si no cambió nada que afecte a la imagen. Añade `[assign.rebuild_triggers]` para condicionar la reconstrucción a archivos específicos:

```toml
[assign.services]
worker = "rebuild"

[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json", "package-lock.json"]
```

Si ninguno de los archivos trigger cambió entre ramas, Coast omite la reconstrucción y en su lugar hace fallback a un reinicio. Esto evita builds costosos de imágenes en cambios rutinarios de código.

## Resumen

| Optimización | Impacto | Afecta | Cuándo usar |
|---|---|---|---|
| `exclude_paths` | Alto | rsync + git diff | Siempre, en cualquier repo con directorios que tu Coast no necesita |
| Eliminar servicios inactivos | Medio | reinicio de servicios | Cuando `COMPOSE_PROFILES` limita qué servicios se ejecutan |
| Estrategia `"hot"` | Medio | reinicio de servicios | Servicios con file watchers (Vite, webpack, nodemon, air) |
| `rebuild_triggers` | Medio | reconstrucción de imagen | Servicios que usan `"rebuild"` y solo lo necesitan para cambios de infra |

Empieza con `exclude_paths`. Es el cambio de menor esfuerzo y mayor impacto que puedes hacer. Acelera tanto el primer assign (rsync) como cada assign posterior (git diff).
