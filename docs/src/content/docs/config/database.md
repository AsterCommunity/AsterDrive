---
title: "数据库配置"
---

:::tip[这一篇覆盖 `[database]`]
连接哪个数据库、连接池多大、启动失败重试几次。第一次部署不知道选哪种？直接用 SQLite，改起来也方便。
:::

```toml
[database]
url = "sqlite://asterdrive.db?mode=rwc"
pool_size = 10
retry_count = 3
```

## 先选数据库类型

- **SQLite** —— 单机、NAS、个人或小团队，最省心
- **PostgreSQL** —— 你已经在跑 PG，或者想接入现有运维体系
- **MySQL** —— 你已经在用 MySQL，想保持统一

:::tip[第一次部署直接 SQLite]
绝大多数场景 SQLite 够用。规模增长后可通过 AsterDrive 自带的跨数据库迁移工具切换，避免被初始选择限制。
:::

## 选项一览

| 选项 | 默认值 | 作用 |
| --- | --- | --- |
| `url` | `"sqlite://asterdrive.db?mode=rwc"` | 数据库连接字符串 |
| `pool_size` | `10` | 连接池大小 |
| `retry_count` | `3` | 启动阶段连接失败时的重试次数 |

## 常见写法

### SQLite

```toml
url = "sqlite://asterdrive.db?mode=rwc"
```

Docker 里更常见的写法：

```toml
url = "sqlite:///data/asterdrive.db?mode=rwc"
```

### PostgreSQL

```toml
url = "postgres://user:password@localhost:5432/asterdrive"
```

用户名或密码含 `@`、`:`、`/`、`?`、`#` 等保留字符时，推荐使用结构化原始凭据，Forge 会安全注入并编码：

```toml
url = { base_url = "postgres://localhost:5432/asterdrive", username = "RAW_USERNAME", password = "RAW_PASSWORD" }
```

### MySQL

```toml
url = "mysql://user:password@localhost:3306/asterdrive"
```

MySQL 同样支持结构化写法：

```toml
url = { base_url = "mysql://localhost:3306/asterdrive", username = "RAW_USERNAME", password = "RAW_PASSWORD" }
```

结构化模式中的 `username` 和 `password` 是原始值，不要预先 percent-encode；`base_url` 不能再包含 userinfo。AsterDrive 不会把这两个字段写入配置序列化、Debug、health 或 doctor 输出。请限制 `data/config.toml` 的读取权限；在 Kubernetes 中优先通过 Secret 挂载完整配置文件，避免把凭据放进命令行或普通日志。

也可以通过嵌套环境变量注入：

```bash
ASTER__DATABASE__URL__BASE_URL=postgres://database.internal:5432/asterdrive
ASTER__DATABASE__URL__USERNAME=RAW_USERNAME
ASTER__DATABASE__URL__PASSWORD=RAW_PASSWORD
```

## 启动时会自动做什么

每次启动，AsterDrive 都会：

1. 建立数据库连接
2. 自动应用迁移（更新 schema）
3. 继续启动服务

**所以日常升级不需要你手动跑迁移命令。**

## 换数据库后端，不要直接改 `url`

:::caution[同种库升级 OK，跨库切换不行]
- 同一种数据库正常升级 —— 直接重启就好，schema 会自动补齐
- 从 SQLite 换到 PostgreSQL/MySQL —— **不能只改 `url` 然后重启**

启动时的自动迁移只负责"更新目标库 schema"，不会把旧数据库的业务数据搬过去。要走这种场景，先用 [运维 CLI](/deployment/ops-cli/) 的 `database-migrate` 把数据迁过去，再切换生产实例。
:::

## SQLite 的路径语义

默认 SQLite 用相对路径时，相对 `data/config.toml` 所在目录解析。

| 部署方式 | 默认落点 |
| --- | --- |
| 本地直接运行 | `./data/asterdrive.db` |
| systemd | `WorkingDirectory/data/asterdrive.db` |
| Docker（写成 `sqlite:///data/asterdrive.db?mode=rwc`） | 容器里的 `/data` |

长期部署，把 SQLite 路径写成固定目录或挂到持久化卷里，避免受工作目录变化影响。

## `pool_size` 和 `retry_count` 怎么调

- 单机、小团队：保持默认
- 外部数据库启动较慢（容器编排里 DB 起得比 AsterDrive 慢）：把 `retry_count` 调高
- 并发高、数据库本身也允许更多连接：再考虑提 `pool_size`

## 对应环境变量

```bash
ASTER__DATABASE__URL="sqlite:///data/asterdrive.db?mode=rwc"
ASTER__DATABASE__POOL_SIZE=10
ASTER__DATABASE__RETRY_COUNT=3
```
