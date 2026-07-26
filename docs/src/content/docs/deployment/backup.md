---
title: "备份与恢复"
---

AsterDrive 当前**不提供统一的 `backup` / `restore` CLI**。  
更稳妥的做法是直接使用数据库和存储后端自己的备份能力，再把恢复后的校验统一交给 `aster_drive doctor`。

这样做的原因很简单：

- SQLite、PostgreSQL、MySQL 都已经有成熟的备份工具和恢复流程
- 本地磁盘、S3 / MinIO 的数据一致性边界并不一样，不适合封装成单一的统一命令
- 只导出数据库，不能替代完整备份；本地对象目录、头像目录和对象存储状态也要一起考虑

## 先分清备份边界

至少要一起保留这些内容：

- `data/config.toml`
- 当前数据库
- 所有本地持久化目录

`config.toml` 里包含登录签名密钥和 MFA/TOTP 加密密钥。恢复时必须使用同一份配置文件，不能只新建一份默认配置再接旧数据库。

:::caution[不要丢失 `auth.mfa_secret_key`]
如果已有用户启用了 MFA，`auth.mfa_secret_key` 丢失或被替换后，原有认证器密钥无法解密。用户会无法用原认证器完成二次验证，只能由管理员逐个重置 MFA 后重新绑定。
:::

这里的“本地持久化目录”通常包括：

- 你在管理端创建的本地存储策略目录，例如 `/data/uploads` 或 `data/uploads`
- `管理 -> 系统设置 -> 用户管理 -> 头像目录` 对应的本地目录；默认是 `data/avatar`
- 你手动配置的其他 `local` 存储策略根目录

如果你使用本地存储，底层 blob 和缩略图都跟着各自的本地存储根目录走，**不要只备份数据库**。

这些目录通常不是长期数据，不建议当成正式备份内容：

- `data/.tmp`
- `data/.uploads`

日志文件要不要一起带走，取决于你的审计、排障和合规要求；它们不是恢复 AsterDrive 运行状态的必要条件。

## 一致性原则

做备份前先记住这几条：

- 最稳妥的做法是安排维护窗口，停掉写入，再同时备份数据库和本地持久化目录
- 如果必须在线备份，优先使用数据库后端自己的在线备份语义，避免依赖手动判断备份时间点
- 不要把“新数据库快照”和“旧对象目录”拼在一起恢复；时间点不一致时，最容易出现数据库引用存在但对象缺失
- `database-migrate` 是跨数据库迁移工具，不是日常备份工具
- `config export` 只能导出运行时系统设置，不能替代完整恢复

## 推荐策略

### SQLite + 本地存储

这是单机、NAS 和大多数 Docker / systemd 部署最常见的场景。

推荐顺序：

1. 停掉 AsterDrive 服务或容器
2. 打包 `data/` 下的持久化内容
3. 如果你还有不在 `data/` 下的本地存储目录或绝对路径头像目录，也一起打包
4. 启动服务

systemd / 直接运行二进制时，常见做法类似：

```bash
sudo systemctl stop asterdrive
sudo tar -C /var/lib/asterdrive \
  --exclude='data/.tmp' \
  --exclude='data/.uploads' \
  -czf /srv/backups/asterdrive-$(date +%F-%H%M%S).tar.gz \
  data
sudo systemctl start asterdrive
```

如果你用了默认 SQLite、本地上传目录和默认头像目录，这个归档通常已经覆盖：

- `data/config.toml`
- `data/asterdrive.db`
- `data/uploads/`（仅当你手动把本地策略配置到这里）
- `data/avatar/`

Docker 部署时，本质上也是同一件事：  
先停容器，再备份挂载卷或 bind mount 的真实目录。

### PostgreSQL / MySQL + 本地存储

这类场景建议把“数据库备份”和“本地目录备份”分开处理：

- PostgreSQL：优先 `pg_dump`、物理备份或你现有的托管备份体系
- MySQL：优先 `mysqldump`、物理备份或你现有的托管备份体系
- 本地存储目录、头像目录：继续用 `tar`、`rsync`、文件系统快照或宿主机备份方案

示例：

```bash
pg_dump \
  --format=custom \
  --file /srv/backups/asterdrive-$(date +%F-%H%M%S).dump \
  "postgres://user:password@127.0.0.1:5432/asterdrive"
```

```bash
mysqldump \
  --single-transaction \
  --routines \
  --events \
  --databases asterdrive \
  > /srv/backups/asterdrive-$(date +%F-%H%M%S).sql
```

如果数据库和本地对象目录不在同一个一致性时间点上，恢复后仍然可能出现引用漂移，所以最好还是在维护窗口里一起做。

### 数据库存外部，对象存储走 S3 / MinIO

这类部署至少还要备份：

- `data/config.toml`
- 数据库
- 本地头像目录（如果你启用了上传头像）

对象数据本身更适合依赖对象存储侧能力：

- 桶版本化
- 生命周期规则
- 跨区域复制
- 托管快照 / 备份

如果你没有给 S3 / MinIO 做版本化或复制，只备份数据库也不算完整方案。

## 恢复顺序

推荐按这个顺序恢复：

1. 停掉 AsterDrive
2. 恢复 `config.toml`
3. 恢复数据库到同一批备份时间点
4. 恢复所有本地持久化目录，或确认对象存储已经切回对应版本
5. 启动 AsterDrive
6. 运行 `doctor` 校验

最少跑一次：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

如果你还恢复了本地对象目录或对象存储，建议继续跑深度检查：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep
```

`--deep` 重点会帮你发现这几类问题：

- `storage-usage`：数据库里的占用统计和真实文件占用不一致
- `blob-ref-counts`：blob 引用计数漂移
- `storage-objects`：对象缺失、未追踪对象、孤儿缩略图
- `folder-tree`：目录结构异常

## 最后再做一件事

不要只做“能备份”，还要定期演练“能恢复”。

更实际的做法是：

- 定一个固定备份频率
- 至少保留一份离线副本
- 定期把备份恢复到测试环境
- 恢复后跑一次 `doctor`
- 再用真实账号登录、上传、下载、分享和恢复回收站项目各做一轮抽查
