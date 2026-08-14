---
description: "AsterDrive 升级与版本迁移：升级前备份、Docker 与 systemd 升级步骤、MySQL 大表注意事项、升级后验证、浏览器缓存处理和回滚方法。"
title: "升级与版本迁移"
---

这一篇覆盖 AsterDrive 的版本升级流程：升级前要备份什么、二进制和 Docker 各自怎么升、升级后怎么验证、出问题怎么回滚。
按你部署方式往下找对应章节即可。

:::caution[升级前先备份]
正式版本会尽量保持升级路径清晰，但升级仍然可能包含数据库 migration、配置项迁移或镜像依赖调整。升级前请完整备份 `config.toml`、数据库和本地存储目录，详见 [备份与恢复](./backup/)。
:::

## 升级前先做这几件事

不管你用哪种部署方式，升级前都该做：

1. **看 [更新日志](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md) 对应版本段** —— 重点看 `Changed` / `Removed` / `Deprecated` 三节
2. **完整备份一次** —— 至少包括 `data/config.toml`、数据库、所有本地存储目录；`config.toml` 里的登录签名密钥和 MFA 加密密钥都要保留，详见 [备份与恢复](./backup/)
3. **确认数据库账号有 DDL 权限** —— 启动时会自动跑 migration，账号没 `CREATE` / `ALTER` 权限会失败
4. **预估停机窗口** —— 小型部署几十秒；如果数据库已有大量数据，看下文 MySQL 章节

如果你管理的是生产环境，建议先在测试环境完整跑一遍升级流程，再正式升级。

## Docker 升级

最常见也最简单的场景。

```bash
# 拉最新镜像
docker pull ghcr.io/astercommunity/asterdrive:latest

# 重启容器
docker compose down
docker compose up -d

# 看启动日志，确认 migration 跑完
docker compose logs -f asterdrive
```

启动日志里会看到 migration 阶段输出。看到 `application started` 之类的提示即可。

如果你用 `docker run` 而不是 compose，记得保持挂载卷不变（`asterdrive-data` 卷 + `config.toml` 挂载点）。

升级后的验证按 [启动后马上检查这些项](./first-check/#启动后马上检查这些项) 走一遍。

## systemd / 二进制升级

```bash
# 1. 停服务
sudo systemctl stop asterdrive

# 2. 备份当前二进制（万一要回滚）
sudo cp /usr/local/bin/aster_drive /usr/local/bin/aster_drive.bak

# 3. 替换二进制
sudo install -m 755 ./aster_drive /usr/local/bin/aster_drive

# 4. 启动
sudo systemctl start asterdrive

# 5. 看日志
sudo journalctl -u asterdrive -f
```

启动时 migration 自动跑。看到服务正常监听端口即可。

如果你想在启动前先单独跑一次 migration（比如想把 migration 报错和服务启动报错分开看），可以用：

```bash
sudo -u asterdrive ./aster_drive database-migrate
```

详见 [运维 CLI](./cli/)。

## MySQL 大表 ALTER 注意事项

:::caution[数据量大的部署需要预留维护窗口]
某些版本的 migration 会对多个表执行 `ALTER TABLE`。数据库会根据自身版本和表结构选择 DDL 算法，部分操作可能触发整表 rebuild 或持有较长时间的 metadata lock。AsterDrive 支持 MySQL 8.0.13 及以上版本，持续集成使用 MySQL 8.4；使用其他受支持的 MySQL 版本时，升级前应按实际数据量演练。完整边界见[数据库配置的版本支持表](/reference/config/database/#%E6%95%B0%E6%8D%AE%E5%BA%93%E7%89%88%E6%9C%AC%E6%94%AF%E6%8C%81)。
:::

如果你的 MySQL 部署数据量较大：

1. **预留维护窗口** —— 停服务、跑 migration、确认完成、启动服务
2. **或者用 online schema change 工具** —— `gh-ost`、`pt-online-schema-change` 等先跑 ALTER，再启动新版本服务

PostgreSQL 和 SQLite 不受这个限制。

后续版本如果再有类似 migration，会在 [更新日志](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md) 里明确标注。

## 升级后验证

升级完成后按这个清单走一遍：

1. `/health` 返回 200
2. `/health/ready` 返回 200，且 `data.status` 是 `ready`（这会检查 DB、默认策略和轻量 driver 前置条件，不代表远端对象存储已完成读写探测）
3. 管理后台能正常打开
4. 用真实账号登录、上传一个文件、下载、分享、恢复一次回收站项目
5. 跑一次 `aster_drive doctor`

如果你启用了 WebDAV / WOPI，再分别验证：

- WebDAV 客户端能挂载、能读写
- WOPI 客户端能打开 Office 文件

这些验证不是流程，是**给自己的安心检查**——确认升级没把哪个边角功能弄坏。

## 浏览器页面与前端资源

AsterDrive 的浏览器页面、公开分享页和服务端由**同一个程序**提供。正常升级就是升级同一个镜像或同一个二进制，页面和服务端会一起更新，不需要单独部署一套静态资源；反向代理也只需要把整个站点代理到 AsterDrive。

如果你不是直接用官方镜像或二进制，而是自己构建或覆盖过页面资源，升级时要保证页面资源和服务端来自同一版本：停服务后一次性替换新的服务端和新的页面资源，再启动并刷新浏览器缓存验收。混用不同版本的页面和服务端会出现"页面还是旧的、服务已经是新的"的半更新状态：

- 页面能打开，但按钮报错
- 新版本功能入口在页面里看不到
- 某些弹窗能打开，但提交失败
- WOPI 或外部预览入口显示异常，或行为和后端版本对不上

升级后页面看起来不对时，按顺序检查：

1. 确认服务已经升级到预期版本
2. 强制刷新浏览器页面
3. Docker 部署确认容器用的是最新镜像
4. 做过自定义页面替换的话，确认页面和服务端来自同一个版本

## 升级失败怎么办

按失败阶段分：

### migration 阶段失败

看日志里的具体报错。常见原因：

- 数据库账号没 DDL 权限 → 给账号加权限再启动
- 之前升级中断、migration 表状态不一致 → 联系开发者前先备份现状（重要）

如果你急需服务恢复，可以先回滚到旧版本（**前提是 migration 没有真的成功执行任何 DDL**）。如果 migration 已经部分执行，回滚旧版本可能因为 schema 不匹配启动失败，必须从备份恢复。

### 启动后某些功能"消失"了

通常不是消失，是位置或名字改了。先看 [更新日志](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md) 对应版本段。如果 changelog 里没有提到，开 issue。

### 启动后行为异常但没报错

按 [故障排查](./troubleshooting/) 处理。

## 回滚

:::danger[跨大版本回滚有数据风险]
如果新版本已经成功跑过 migration 改了 schema，回滚到旧版本通常会启动失败（旧二进制不认新 schema），或者更糟——能启动但数据被静默截断。

**回滚的安全做法是从备份恢复**，不是直接换回旧二进制。
:::

回滚步骤：

1. 停服务
2. 从备份恢复 `config.toml`、数据库、本地存储目录到升级前的时间点
3. 替换回旧二进制
4. 启动
5. 跑 `aster_drive doctor` 确认状态

详见 [备份与恢复](./backup/#恢复顺序)。

## 从旧版本升级

当前版本的正式升级路径以 `v0.1.0` 及之后的 migration 历史为准。也就是说，数据库的 `seaql_migrations` 表中应当包含当前基线迁移记录：

```text
m20260512_000001_baseline_schema
```

如果你从 `v0.1.0` 或之后的版本升级，按本文前面的 Docker / systemd 步骤执行即可；服务启动时会自动应用后续 migration。

早期 alpha / beta / rc 预发布构建使用过已经重排的历史 migration。当前版本不再内置这些 rebase 兼容分支，也不会把旧的 migration 记录自动改写成当前 baseline。仍停留在早期预发布历史上的实例，需要先升级到能完成当时 rebase 的中间版本并确认迁移完成，再继续升级；或者从已经完成当前 baseline 后的备份恢复。

不要手工修改 `seaql_migrations`，也不要为了绕过 migration 报错去清空业务表。迁移元数据和真实业务表结构不一致时，直接启动新版本可能造成更难恢复的数据问题。

:::tip[拿不准当前库在哪一段]
先备份现状，再用当前版本的 `aster_drive doctor --database-url ...` 看 `Database migrations` 检查项。它会列出未知 migration 记录或待执行的当前 migration。
:::
