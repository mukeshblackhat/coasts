# 端口

Coast 会为 Coast 实例中的每个服务管理两种端口映射:规范端口和动态端口。

## 规范端口

这些是你的项目通常运行使用的端口——也就是你在 `docker-compose.yml` 或本地开发配置中的那些端口。例如，Web 服务器的 `3000`，Postgres 的 `5432`。

同一时间只能有一个 Coast 拥有规范端口。哪个 Coast 被[检出](CHECKOUT.md)，哪个就会获得这些端口。

```text
coast checkout dev-1

localhost:3000  ──→  dev-1
localhost:5432  ──→  dev-1
```

这意味着你的浏览器、API 客户端、数据库工具和测试套件都能像平常一样正常工作——无需更改端口号。

在 Linux 上，低于 `1024` 的规范端口在 [`coast checkout`](CHECKOUT.md) 绑定它们之前，可能需要先进行主机配置。动态端口没有这个限制。

## 动态端口

每个正在运行的 Coast 都始终会在高位范围（49152–65535）中获得自己的一组动态端口。这些端口会被自动分配，并且始终可访问，而不受哪个 Coast 被检出的影响。

```text
coast ports dev-1

# SERVICE    CANONICAL  DYNAMIC
# ★ web      3000       62217
#   db       5432       55681

coast ports dev-2

# SERVICE    CANONICAL  DYNAMIC
# ★ web      3000       63104
#   db       5432       57220
```

动态端口让你无需检出就能查看任何 Coast。你可以打开 `localhost:63104` 来访问 dev-2 的 Web 服务器，即使此时 dev-1 已被检出并占用了规范端口。

## 它们如何协同工作

```text
┌──────────────────────────────────────────────────┐
│  Your machine                                    │
│                                                  │
│  Canonical (checked-out Coast only):             │
│    localhost:3000 ──→ dev-1 web                  │
│    localhost:5432 ──→ dev-1 db                   │
│                                                  │
│  Dynamic (always available):                     │
│    localhost:62217 ──→ dev-1 web                 │
│    localhost:55681 ──→ dev-1 db                  │
│    localhost:63104 ──→ dev-2 web                 │
│    localhost:57220 ──→ dev-2 db                  │
└──────────────────────────────────────────────────┘
```

切换[检出](CHECKOUT.md)是即时的——Coast 会终止并重新生成轻量级的 `socat` 转发器。不会重启任何容器。

另请参阅 [Primary Port & DNS](PRIMARY_PORT_AND_DNS.md) 了解快速链接、子域路由和 URL 模板。
