---
description: "AsterDrive 运维 CLI 参考：doctor 部署检查、离线系统设置、远程节点 enroll 和跨数据库迁移四个任务的命令用法。"
title: "运维 CLI"
---

:::tip[这一篇覆盖什么]
`aster_drive` 可执行文件除了启动服务，还带一组命令行子命令：`doctor`（部署检查）、`config`（离线系统设置）、`node`（远程节点接入）、`database-migrate`（跨数据库迁移）。
**日常改设置优先走管理后台**——这一篇适合的是"后台进不去"、"想纳入脚本"、"要换数据库后端"这类场景。
:::

AsterDrive 现在自带一组命令行工具，适合下面这些场景：

- 服务已经部署好，但你想先离线检查一遍数据库和关键设置
- 管理后台暂时进不去，需要直接查看或修改系统设置
- 需要在从节点 shell 上完成 `node enroll`
- 准备把 SQLite 迁到 PostgreSQL 或 MySQL，或者反过来迁回去
- 想把检查结果交给脚本、CI 或运维平台处理

这些命令都还是同一个 `aster_drive` 可执行文件。  
直接运行 `./aster_drive` 是启动服务；带子命令时，就是执行运维操作。

## 命令速查

| 你要做什么 | 用哪个命令 | 什么时候用 |
| --- | --- | --- |
| 检查数据库、迁移、公开站点地址、邮件、默认策略 | `doctor` | 新部署、上线前、升级后 |
| 深度核对容量、Blob 引用、对象清单、目录树 | `doctor --deep` | 怀疑数据和存储不一致 |
| 自动修复部分计数漂移 | `doctor --deep --fix` | 已确认可以让 CLI 改计数 |
| 离线查看系统设置 | `config list` / `config get` | 后台进不去，或想纳入脚本 |
| 离线修改系统设置 | `config set` / `config import` | 停机窗口、批量配置、灾难恢复 |
| 在 follower 上完成接入 | `node enroll` | 主控后台生成 enroll 命令后 |
| SQLite / PostgreSQL / MySQL 之间迁移 | `database-migrate` | 换数据库后端或做迁移演练 |

:::caution[先备份再让 CLI 写数据]
`doctor --fix`、`config set`、`config import`、`database-migrate` 都可能改变数据库。正式环境先备份，避免在未备份的情况下执行试操作，导致后续需要恢复事故。
:::

## 先准备数据库地址

最常见的写法：

```text
sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc
postgres://user:password@127.0.0.1:5432/asterdrive
mysql://user:password@127.0.0.1:3306/asterdrive
```

如果你用的是官方 Docker 容器，最省事的做法通常是先进入容器，再跑这些命令：

```bash
docker exec -it asterdrive sh
```

这样 SQLite 路径、挂载卷和容器里的实际文件位置不会搞混。

## 用环境变量代替常用参数

运维脚本里如果不想每条命令都写很长的参数，可以用 CLI 专用的 `ASTER_CLI_*` 环境变量。它们只影响命令行工具，不会改服务启动配置。

| 环境变量 | 对应参数 | 适用命令 |
| --- | --- | --- |
| `ASTER_CLI_DATABASE_URL` | `--database-url` | `doctor`、`config`、`node enroll` |
| `ASTER_CLI_OUTPUT_FORMAT` | `--output-format` | `doctor`、`config`、`node`、`database-migrate` |
| `ASTER_CLI_MASTER_URL` | `node enroll --master-url` | `node enroll` |
| `ASTER_CLI_ENROLLMENT_TOKEN` | `node enroll --token` | `node enroll` |
| `ASTER_CLI_SOURCE_DATABASE_URL` | `database-migrate --source-database-url` | `database-migrate` |
| `ASTER_CLI_TARGET_DATABASE_URL` | `database-migrate --target-database-url` | `database-migrate` |

`doctor` 还有这些脚本友好的开关：

```bash
ASTER_CLI_DOCTOR_STRICT=true
ASTER_CLI_DOCTOR_DEEP=true
ASTER_CLI_DOCTOR_FIX=true
ASTER_CLI_DOCTOR_SCOPE=blob-ref-counts,storage-objects
ASTER_CLI_DOCTOR_POLICY_ID=3
```

数据库迁移时，如果需要观察进度或调小批次，也可以用：

```bash
ASTER_CLI_PROGRESS=1
ASTER_CLI_COPY_BATCH_SIZE=100
```

:::tip[服务配置和 CLI 参数是两套 ENV]
`ASTER__DATABASE__URL` 会覆盖服务读取的 `config.toml`；`ASTER_CLI_DATABASE_URL` 只给运维 CLI 当参数。

如果你是在同一个 shell 里同时启动服务和跑命令，建议把两类变量分开放清楚，排查时会轻松很多。
:::

## 部署检查：`doctor`

第一次部署完成后，或者准备上线前，最值得先跑一次：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

默认模式会检查这些最容易出问题的地方：

- 数据库能不能连上
- 数据库里还有没有待执行迁移
- 如果后端是 SQLite，`FTS5 + trigram tokenizer` 的搜索加速能力是否可用，相关 FTS 表 / 触发器是否齐全
- 运行时系统设置能不能正常读出
- `公开站点地址` 是否为空或格式不对
- `公开站点地址` 是否仍然是 `http://`，导致正式上线缺少 HTTPS
- 邮件投递配置是否完整
- 预览应用配置是否能正常解析
- 默认存储策略和默认策略组是否已经准备好

如果你希望把 `warn` 也当成失败处理，可以加：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --strict
```

需要给脚本处理时，再补一个输出格式：

```bash
./aster_drive doctor \
  --output-format json \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

最适合拿它来做这些事：

- 新部署后的首轮验收
- 升级后补一轮健康检查
- 改完 `公开站点地址`、邮件或预览应用后，确认没有把配置改坏
- 确认默认 SQLite 部署真的带上了搜索加速，而不是悄悄退回全表扫描

如果你怀疑库里已经有“数据和存储不一致”的问题，可以再跑深度检查：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep
```

`--deep` 额外会做这些检查：

- `storage-usage`：核对 `users.storage_used` / `teams.storage_used` 和文件、历史版本实际占用
- `blob-ref-counts`：核对 `file_blobs.ref_count` 与 `files` / 非 current `file_revisions` 的真实引用数
- `storage-objects`：扫描每个存储策略下的对象路径，找出缺失 Blob、未追踪对象和孤儿缩略图
- `folder-tree`：检查缺失父目录、跨工作空间父目录和目录循环引用
- `revision-ledger`：检查 history/head/predecessor 指针和 `files` current projection

如果你只想跑其中一部分，可以直接缩小范围：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --scope blob-ref-counts,storage-objects
```

如果你只想检查某个存储策略，可以再加：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --scope storage-objects \
  --policy-id 3
```

发现计数漂移时，可以让 CLI 直接修：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep \
  --fix
```

这里要注意四件事：

- `--scope` 只影响 deep checks，不会关闭数据库连接、迁移、运行时配置这些基础检查
- `--policy-id` 只作用于 `blob-ref-counts` 和 `storage-objects`；`storage-usage` 与 `folder-tree` 仍按全库核对
- `--fix` 目前只会修复 `storage_used` 和 `file_blobs.ref_count` 两类计数，不会自动删对象或改目录结构
- 深度扫描会按数据库批次和对象存储分页执行，但它只校验路径级存在性，不会读取对象内容或做 checksum

## 离线系统设置：`config`

平时改设置，还是优先走 `管理 -> 系统设置`。  
`config` 更适合下面这些情况：

- 后台暂时进不去
- 维护窗口里不想开网页操作
- 想批量导出、校验或导入系统设置

先看当前有哪些项：

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  list
```

只看某一项：

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  get \
  --key public_site_url
```

先校验，再落库：

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  validate \
  --key public_site_url \
  --value '["https://drive.example.com"]'

./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  set \
  --key public_site_url \
  --value '["https://drive.example.com"]'
```

`public_site_url` 支持多个公开来源。命令行写入时传 JSON 字符串数组：

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  set \
  --key public_site_url \
  --value '["https://drive.example.com","https://panel.example.com"]'
```

批量导入时，输入文件可以是下面两种 JSON 之一：

```json
[
  { "key": "public_site_url", "value": ["https://drive.example.com", "https://panel.example.com"] },
  { "key": "auth_cookie_secure", "value": "true" }
]
```

```json
{
  "configs": [
    { "key": "public_site_url", "value": ["https://drive.example.com", "https://panel.example.com"] },
    { "key": "auth_cookie_secure", "value": "true" }
  ]
}
```

导入示例：

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  import \
  --input-file ./runtime-config.json
```

导出现有配置时，可以这样做：

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --output-format pretty-json \
  export
```

导出结果更适合审阅、备份或交给脚本处理。  
如果你打算重新导入，先把它整理成上面那种“键值数组”或 `{"configs": [...]}` 结构，再交给 `import`。

如果你只是想确认某个值是否合法，优先用 `validate`，不要直接使用 `set`。

## 远程节点接入：`node enroll`

这个命令只给从节点用。主控后台生成 enroll token 后，在 follower 机器上执行：

```bash
./aster_drive node enroll \
  --master-url https://drive.example.com \
  --token enr_xxxxx
```

如果你没有显式传数据库地址，命令会读取当前 `data/config.toml` 里的 `[database].url`。也可以直接指定：

```bash
./aster_drive node enroll \
  --master-url https://drive.example.com \
  --token enr_xxxxx \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

它会做这些事：

- 确认当前配置是 `[server].start_mode = "follower"`
- 当前目录还没有配置文件时，先按 follower 模式生成一份默认 `data/config.toml`
- 用 token 向主控兑换本地绑定信息，并写入 follower 数据库
- 输出当前监听地址、配置文件路径和下一步连通性检查提示

命令本身不会替你创建主控节点的默认远程存储目标，也不会启动 HTTP 服务。执行成功后，重启 follower 进程，再回主控后台创建或应用默认远程存储目标。

Docker follower 更推荐用启动环境变量自动 enroll，不需要进容器手动跑这条命令，见 [Docker 部署从节点](/deploy/follower-node/)。

## 跨数据库迁移：`database-migrate`

这个命令是给“换数据库后端”用的。  
它不是日常启动时的自动 schema 迁移，而是把现有业务数据从一个数据库搬到另一个数据库。

最常见的场景：

- SQLite 迁到 PostgreSQL
- SQLite 迁到 MySQL
- PostgreSQL 和 MySQL 之间做后端切换

推荐顺序：

1. 先做一次 `--dry-run`
2. 准备停机窗口，避免源库在迁移过程中继续写入
3. 正式执行迁移
4. 看到 `ready_to_cutover = true` 后，再把生产实例切到新数据库

先试跑：

```bash
./aster_drive database-migrate \
  --source-database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --target-database-url "postgres://user:password@127.0.0.1:5432/asterdrive_new" \
  --dry-run
```

正式执行：

```bash
./aster_drive database-migrate \
  --source-database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --target-database-url "postgres://user:password@127.0.0.1:5432/asterdrive_new"
```

只做目标库校验：

```bash
./aster_drive database-migrate \
  --source-database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --target-database-url "postgres://user:password@127.0.0.1:5432/asterdrive_new" \
  --verify-only
```

这个命令当前会自动处理这些事：

- 检查源库和目标库的迁移状态
- 自动把目标库 schema 补到当前版本
- 按固定顺序复制业务表
- 做行数、唯一约束和外键校验
- 在目标库里写入 checkpoint，命令中断后可以用同一条命令继续跑

用它时要记住三件事：

- 源库必须先是“当前 schema”，有待执行迁移就会直接拒绝
- 迁移过程中不要继续往源库写新数据
- 只有报告里的 `ready_to_cutover = true`，才说明目标库已经达到可切换状态

## 什么时候优先看这页

- 部署完成，但还没放心上线
- 后台打不开，又急着查配置
- 从节点已经拿到 enroll token，要在 shell 里完成接入
- 准备从 SQLite 切到 PostgreSQL / MySQL
- 想把检查动作做成脚本
