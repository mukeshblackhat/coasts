# 성능 최적화

Coast는 브랜치 전환을 빠르게 만들도록 설계되었지만, 대규모 모노레포에서는 기본 동작이 불필요한 지연을 유발할 수 있습니다. 이 페이지에서는 Coastfile에서 `assign` 및 `unassign` 시간을 줄이기 위해 사용할 수 있는 조정 수단을 다룹니다.

## Assign이 느릴 수 있는 이유

`coast assign`는 Coast를 새로운 worktree로 전환할 때 여러 작업을 수행합니다:

```text
coast assign dev-1 --worktree feature/payments

  1. stop affected compose services
  2. create git worktree (if new)
  3. sync gitignored files into worktree (rsync)  ← often the bottleneck
  4. remount /workspace
  5. git ls-files diff  ← can be slow in large repos
  6. restart/rebuild services
```

지연의 대부분은 두 단계에서 발생합니다: **gitignored 파일 동기화**와 **`git ls-files` diff**입니다. 둘 다 저장소 크기에 비례해 증가하며, macOS VirtioFS 오버헤드로 인해 더 증폭됩니다.

### Gitignored 파일 동기화

worktree가 처음 생성될 때 Coast는 `rsync --link-dest`를 사용해 gitignored 파일(빌드 산출물, 캐시, 생성 코드)을 프로젝트 루트에서 새 worktree로 하드링크합니다. 하드링크는 파일당 거의 즉시 처리되지만, rsync는 여전히 동기화가 필요한 항목을 찾기 위해 소스 트리의 모든 디렉터리를 순회해야 합니다.

프로젝트 루트에 rsync가 건드리지 말아야 할 큰 디렉터리(다른 worktree, vendor된 의존성, 관련 없는 앱)가 포함되어 있으면, rsync는 실제로는 복사하지 않을 수천 개 파일의 디렉터리로 내려가며 `stat`을 수행하느라 시간을 낭비합니다. 400,000개 이상의 gitignored 파일이 있는 저장소에서는 이 순회만으로도 30–60초가 걸릴 수 있습니다.

Coast는 이 동기화에서 `node_modules`, `.git`, `dist`, `target`, `.worktrees`, `.coasts` 및 기타 흔히 무거운 디렉터리를 자동으로 제외합니다. 추가 디렉터리는 Coastfile의 `exclude_paths`를 통해 제외할 수 있습니다(아래 참고).

worktree가 한 번 동기화되면 `.coast-synced` 마커가 기록되며, 이후 동일 worktree로의 assign은 동기화를 완전히 건너뜁니다.

### `git ls-files` Diff

모든 assign과 unassign은 브랜치 간 변경된 추적 파일을 판단하기 위해 `git ls-files`도 실행합니다. macOS에서는 호스트와 Docker VM 사이의 모든 파일 I/O가 VirtioFS(또는 구형 설정에서는 gRPC-FUSE)를 통과합니다. `git ls-files` 작업은 모든 추적 파일에 대해 `stat`을 수행하며, 파일당 오버헤드가 빠르게 누적됩니다. 추적 파일이 30,000개인 저장소는 실제 diff가 작더라도 5,000개인 저장소보다 체감상 훨씬 오래 걸립니다.

## `exclude_paths` — 주요 레버

Coastfile의 `exclude_paths` 옵션은 **gitignored 파일 동기화**(rsync)와 **`git ls-files` diff** 모두에서 전체 디렉터리 트리를 건너뛰도록 Coast에 지시합니다. 제외된 경로 아래의 파일은 여전히 worktree에 존재하지만, assign 중 순회 대상에서만 제외됩니다.

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

이는 대규모 모노레포에서 가장 영향력이 큰 단일 최적화입니다. 첫 assign 시의 rsync 순회와, 매 assign 시의 파일 diff 모두를 줄여줍니다. 프로젝트에 추적 파일이 30,000개 있지만 Coast에서 실행 중인 서비스에 관련된 파일이 20,000개뿐이라면, 나머지 10,000개를 제외함으로써 매 assign에서 수행해야 할 작업의 1/3을 줄일 수 있습니다.

### 무엇을 제외할지 선택하기

목표는 Coast 서비스가 필요로 하지 않는 모든 것을 제외하는 것입니다. 먼저 저장소에 무엇이 있는지 프로파일링하세요:

```bash
git ls-files | cut -d'/' -f1 | sort | uniq -c | sort -rn
```

이 명령은 최상위 디렉터리별 파일 수를 보여줍니다. 그다음 compose 서비스가 실제로 마운트하거나 의존하는 디렉터리를 식별하고, 나머지는 제외하세요.

**포함(Keep)** 해야 할 디렉터리:
- 실행 중인 서비스에 마운트되는 소스 코드를 포함하는 디렉터리(예: 앱 디렉터리)
- 해당 서비스가 임포트하는 공유 라이브러리를 포함하는 디렉터리
- `[assign.rebuild_triggers]`에서 참조되는 디렉터리

**제외(Exclude)** 해야 할 디렉터리:
- Coast에서 실행되지 않는 앱/서비스에 속한 디렉터리(다른 팀의 앱, 모바일 클라이언트, CLI 도구)
- 런타임과 무관한 문서, 스크립트, CI 설정, 또는 툴링을 포함하는 디렉터리
- 저장소에 체크인된 대용량 의존성 캐시(예: vendor된 proto 정의, `.yarn` 오프라인 캐시)

### 예시: 여러 앱이 있는 모노레포

여러 앱에 걸쳐 29,000개 파일이 있지만 그중 두 개만 관련 있는 모노레포:

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

이렇게 하면 diff 범위가 29,000개 파일에서 약 21,000개로 줄어들어, 매 assign에서 `stat` 횟수가 대략 28% 감소합니다.

## `[assign.services]`에서 비활성 서비스를 제거하기

`COMPOSE_PROFILES`가 일부 서비스만 시작한다면, `[assign.services]`에서 비활성 서비스를 제거하세요. Coast는 목록에 있는 모든 서비스에 대해 assign 전략을 평가하며, 실행 중이 아닌 서비스를 재시작하거나 리빌드하는 것은 낭비입니다.

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

`[assign.rebuild_triggers]`에도 동일하게 적용됩니다 — 활성화되지 않은 서비스에 대한 항목은 제거하세요.

## 가능한 경우 `"hot"` 사용하기

`"hot"` 전략은 컨테이너 재시작 자체를 완전히 건너뜁니다. [filesystem remount](FILESYSTEM.md)가 `/workspace` 아래의 코드를 교체하면 서비스의 파일 워처(Vite, webpack, nodemon, air 등)가 변경 사항을 자동으로 감지합니다.

```toml
[assign.services]
web = "hot"        # Vite/webpack dev server with HMR
api = "restart"    # Rails/Go — needs a process restart
```

`"hot"`은 컨테이너 stop/start 사이클을 피하기 때문에 `"restart"`보다 빠릅니다. 파일 감시가 있는 개발 서버를 실행하는 서비스에 사용하세요. 시작 시에만 코드를 로드하고 변경을 감시하지 않는 서비스(대부분의 Rails, Go, Java 앱)는 `"restart"`를 사용하세요.

## 트리거와 함께 `"rebuild"` 사용하기

서비스의 기본 전략이 `"rebuild"`라면, 브랜치를 전환할 때마다 Docker 이미지가 리빌드됩니다 — 이미지에 영향을 주는 변경이 전혀 없더라도 말입니다. `[assign.rebuild_triggers]`를 추가해 특정 파일에 따라 리빌드를 게이트하세요:

```toml
[assign.services]
worker = "rebuild"

[assign.rebuild_triggers]
worker = ["Dockerfile", "package.json", "package-lock.json"]
```

브랜치 간에 트리거 파일이 하나도 변경되지 않았다면, Coast는 리빌드를 건너뛰고 대신 재시작으로 폴백합니다. 이는 일상적인 코드 변경에서 비용이 큰 이미지 빌드를 피하게 해줍니다.

## 요약

| Optimization | Impact | Affects | When to use |
|---|---|---|---|
| `exclude_paths` | 높음 | rsync + git diff | Coast에 필요 없는 디렉터리가 있는 모든 저장소에서 항상 |
| 비활성 서비스 제거 | 중간 | service restart | `COMPOSE_PROFILES`가 실행 서비스를 제한할 때 |
| `"hot"` 전략 | 중간 | service restart | 파일 워처가 있는 서비스(Vite, webpack, nodemon, air) |
| `rebuild_triggers` | 중간 | image rebuild | `"rebuild"`를 사용하는 서비스 중 인프라 변경에만 필요할 때 |

`exclude_paths`부터 시작하세요. 이는 가장 적은 노력으로 가장 큰 효과를 내는 변경입니다. 첫 assign(rsync)과 이후의 모든 assign(git diff)을 모두 빠르게 합니다.
