# 测试与数据库后端

本文描述的是当前仓库已经落地的测试后端切换机制，不是未来规划。

## 先说结论

- 默认集成测试仍然跑内存 SQLite
- 现在可以通过 `ASTER_TEST_DATABASE_BACKEND` 把 `tests/common/mod.rs` 里的通用 `common::setup()` 切到 PostgreSQL 或 MySQL
- PostgreSQL / MySQL 不是要求你手填数据库 URL；测试会自动用 `testcontainers` 起容器
- 为了支持并行测试，PostgreSQL / MySQL 下每个测试实例都会分配独立数据库，不共用同一个 schema

## 环境变量

支持三个值：

- `sqlite`
- `postgres`
- `mysql`

未设置时等价于：

```bash
ASTER_TEST_DATABASE_BACKEND=sqlite
```

## 运行方式

默认 SQLite：

```bash
cargo test
```

切到 PostgreSQL：

```bash
ASTER_TEST_DATABASE_BACKEND=postgres cargo test
```

切到 MySQL：

```bash
ASTER_TEST_DATABASE_BACKEND=mysql cargo test
```

如果你只想复现某一组用例，和平时一样筛测试名即可：

```bash
ASTER_TEST_DATABASE_BACKEND=postgres cargo test --test test_search test_search_by_name
ASTER_TEST_DATABASE_BACKEND=mysql cargo test --test test_admin test_admin_team_crud
```

## 现在的行为

`tests/common/mod.rs` 里的 `common::setup()` 会按下面的规则工作：

1. 读取 `ASTER_TEST_DATABASE_BACKEND`
2. 如果是 `sqlite`，直接返回内存 SQLite 的 `AppState`
3. 如果是 `postgres` 或 `mysql`，通过 `testcontainers` 启动一个共享容器
4. 基于容器里的基础数据库，为当前测试实例创建一个唯一数据库名
5. 用这个唯一数据库跑 migration、初始化默认策略和运行时配置，再返回 `AppState`

这意味着：

- PostgreSQL / MySQL 共享容器会尽量跨多次本地测试命令复用，不会每次都重新冷启动
- 但数据库实例不会复用，所以并行集成测试不会互相污染数据
- 已退出测试进程留下的独立数据库，会在下一次启动对应后端测试容器时自动清理

## PostgreSQL / MySQL 的差异

### PostgreSQL

- 使用容器内的 `postgres` 管理账号启动基础库
- 测试实例数据库也由这个管理连接创建
- 业务测试连接直接使用对应的测试数据库

### MySQL

- 业务测试默认仍使用容器内的 `aster` 用户
- 独立数据库仍由 `root` 连接创建
- 但普通测试用户的访问权限会在容器启动时一次性补齐，不再为每个测试库单独跑一次 `GRANT`

## 什么时候该切后端

下面这些情况，别只拿 SQLite 自我安慰：

- 你刚改了 repo 层查询，而且里面有数据库分支逻辑
- 你刚改了全文搜索、索引、分页、排序、大小写匹配
- 你怀疑某段 SQL / SeaORM builder 在 PostgreSQL 或 MySQL 上行为不一致
- 你要修的是“SQLite 绿，生产库炸”的问题

更实际一点的建议：

- 先用默认 SQLite 快速迭代
- 改到数据库相关逻辑后，至少再补跑一次 `postgres`
- 如果代码里还有 MySQL 分支，再补跑一次 `mysql`

## 和现有 smoke tests 的关系

仓库里还有 [tests/test_database_backends.rs](../../../tests/test_database_backends.rs)，它的定位没有变：

- 主要负责生产数据库相关的 smoke coverage
- 会显式验证 PostgreSQL / MySQL 搜索索引、搜索链路和跨库迁移路径
- 它是专门的后端 smoke 覆盖，不是唯一会跑多数据库的测试入口；普通集成测试只要走 `common::setup()`，也可以用 `ASTER_TEST_DATABASE_BACKEND` 切后端

新的 `ASTER_TEST_DATABASE_BACKEND` 机制解决的是另一件事：

- 让原本默认写成 `common::setup()` 的大部分集成测试，可以在不改测试主体的情况下切到其他后端复跑

## 限制和注意事项

- PostgreSQL / MySQL 依赖本机可用的 Docker / 容器运行时
- 第一次跑会拉镜像，明显比 SQLite 慢
- 之后同一个工作区重复跑 `postgres` / `mysql` 测试，通常会直接复用已有共享容器，冷启动开销会小很多
- 如果某个测试没有走 `common::setup()`，而是自己手写数据库初始化逻辑，那它不会自动吃到这个开关
- `common::setup_with_database_url(...)` 仍然保留给需要显式控制数据库地址的场景，它不会替你解读 `ASTER_TEST_DATABASE_BACKEND`

## 排查建议

如果你怀疑测试没有按预期切后端，优先看三件事：

1. 当前用例是不是走的 `common::setup()`
2. shell 里是否真的导出了 `ASTER_TEST_DATABASE_BACKEND`
3. 本机 Docker 是否可用，以及对应镜像能否拉起

## SFTP 集成测试

SFTP 驱动有单独的集成测试：

```bash
cargo test --test test_sftp
```

这个测试默认会通过 `testcontainers` 启动 `lscr.io/linuxserver/openssh-server` 容器，完成一次真实上传、下载、range 读取、删除和主机密钥指纹确认流程。它需要本机 Docker / 容器运行时可用。

如果当前环境不能跑 Docker，可以显式关闭：

```bash
ASTER_SFTP_TEST_DOCKER=0 cargo test --test test_sftp
```

关闭后测试会跳过容器 round-trip。不要把这个变量作为默认 CI 行为；SFTP 是真实存储驱动，PR 改到驱动、connector、descriptor 或上传下载链路时应优先保留默认 Docker 测试。

`src/storage/drivers/sftp.rs` 里还有一个手动真实服务器用例，需要 `ASTER_SFTP_TEST_*` 和 `ASTER_SFTP_TEST_HOST_KEY_FINGERPRINT`。它不替代 `tests/test_sftp.rs` 的默认 Docker 覆盖，主要用于排查特定 SFTP 服务器兼容性。
