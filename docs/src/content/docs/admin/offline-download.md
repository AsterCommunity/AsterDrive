---
description: "AsterDrive 离线下载（链接导入）管理配置：引擎注册表、运行时设置、临时目录语义、aria2 接入步骤、安全边界和常见故障处理。"
title: "离线下载（链接导入）"
---

:::tip[这一篇讲什么]
管理员视角的链接导入配置：下载引擎、限额、临时目录和 aria2 接入。
用户怎么提交链接导入任务，见 [上传与下载](/using/upload-download/)。
:::

离线下载是文件页面里的"从链接导入"能力：用户提交一个 HTTP/HTTPS 下载地址，AsterDrive 在服务端创建后台任务，把文件下载到临时目录，校验后导入到个人空间或团队空间。

:::tip[名字别混]
界面上通常叫"链接导入"，配置键和任务类型里叫 `offline_download`。这一页把两者当成同一个功能说明。
:::

## 安全边界

AsterDrive 会在派发任务前做基础防护：

- 只接受 HTTP/HTTPS URL
- 不跟随 HTTP 重定向；如果源站返回重定向，请改用最终真实下载地址
- 拒绝解析到本机、私网、链路本地、多播、文档保留地址和云厂商元数据地址的主机
- 内置下载器流式写入临时文件，不会把整文件先读进内存
- 下载完成后才进入 SHA-256 校验和导入工作空间

如果启用 aria2，AsterDrive 仍会先做这些 URL 校验，但实际 DNS 解析和出站连接由 aria2 daemon 执行。生产环境应在网络层隔离 aria2，并限制 JSON-RPC 端点只允许 AsterDrive 访问。

## 引擎注册表

离线下载引擎由 `offline_download_engine_registry_json` 控制。它是一个有序注册表，当前支持：

- `builtin`：AsterDrive 内置下载器
- `aria2`：管理员维护的 aria2 JSON-RPC 下载器

默认注册表启用 `builtin`，关闭 `aria2`。启用多个引擎时，任务会按注册表顺序尝试；如果前一个引擎失败，会尝试下一个启用的引擎。全部关闭时，新的链接导入任务会被拒绝，这可以作为维护窗口里的显式关闭开关。

典型配置：

```json
{
  "version": 1,
  "engines": [
    {
      "kind": "aria2",
      "enabled": true
    },
    {
      "kind": "builtin",
      "enabled": true
    }
  ]
}
```

任务详情会展示实际完成任务的下载器；aria2 引擎运行时还会把 GID 写入内部 `runtime_json`，用于诊断和恢复边界。

## 运行时设置

在 `管理 -> 系统设置 -> 文件处理 -> 链接导入` 里可以调整这些设置：

| 设置 | 默认值 | 说明 |
| --- | --- | --- |
| 链接导入引擎注册表 | `builtin` 启用，`aria2` 关闭 | 决定启用哪些下载器以及兜底顺序 |
| 链接导入文件大小上限 | `1 GiB` | 服务端允许下载的最大源文件大小 |
| 链接导入下载速度上限 | `5` MB/s | 单个任务的最大平均速度；`0` 表示不限制 |
| 链接导入任务并发上限 | `1` | 同时允许运行多少个链接导入任务 |
| 链接导入请求超时 | `600` 秒 | 完整下载允许持续的时间 |
| 离线下载临时目录 | 空，使用服务默认临时目录 | 可选绝对路径；AsterDrive 和外部下载器必须都能用同一个路径访问 |
| aria2 RPC 地址 | 空 | 只在 aria2 引擎启用时使用 |
| aria2 RPC 密钥 | 空 | 敏感配置；读取时脱敏 |
| aria2 RPC 请求超时 | `10` 秒 | 单次 JSON-RPC 调用超时，不替代完整下载超时 |
| aria2 split | `5` | 每个任务传给 aria2 的 `split` |
| aria2 单服务器连接数 | `5` | 每个任务传给 aria2 的 `max-connection-per-server` |
| aria2 最低速度阈值 | `0` | 传给 aria2 的 `lowest-speed-limit`；`0` 表示关闭 |

AsterDrive 不透传任意 aria2 options，只暴露上面这几个管理员控制的安全子集。链接导入速度上限会映射为 aria2 的单任务 `max-download-limit`，不是 daemon 全局限速。

## 临时目录语义

`offline_download_temp_dir` 是离线下载的 staging 根目录。留空时使用服务默认临时目录；填写时必须是绝对路径。

AsterDrive 会在这个目录下创建 `tasks/{task_id}/{processing_token}/source`。内置下载器由 AsterDrive 自己写入这个文件；aria2 引擎则会把同一个目录和文件名通过 JSON-RPC 发给 aria2。因此这个路径不是"宿主机路径映射表"，而是 AsterDrive 和 aria2 双方看到的同一个路径字符串。

部署时要保证：

- AsterDrive 进程对目录有读、写、执行权限
- 外部下载器（如 aria2）也能访问并写入同一个绝对路径
- 目录只存放任务临时产物，可以排除在备份之外

全 Docker 部署建议设置为 `/data/.tmp/offline-download`，并把同一个宿主机 `./data` 挂到 AsterDrive 和 aria2 的 `/data`。宿主机 `cargo run` + Compose aria2 的混合模式，可以设置为宿主机绝对路径，例如 `/srv/asterdrive/offline-download-temp`，并把 aria2 容器也挂载到同一个容器内绝对路径：

```yaml
volumes:
  - ./data/offline-download-temp:/srv/asterdrive/offline-download-temp
```

## 启用 aria2

Docker 部署里可以用仓库根目录的 `aria2` profile 启动 aria2：

```bash
mkdir -p ./data ./aria2-config
sudo chown -R 10001:10001 ./data ./aria2-config

export ASTERDRIVE_ARIA2_RPC_SECRET="$(openssl rand -hex 24)"
docker compose --profile aria2 up -d
```

全 Docker 部署时，AsterDrive 和 aria2 必须把同一个宿主机 `./data` 挂到容器内同一个 `/data` 路径，因为 AsterDrive 会把任务临时文件路径传给 aria2。建议把 `offline_download_temp_dir` 设置为 `/data/.tmp/offline-download`。

然后在系统设置里配置：

| 场景 | `offline_download_aria2_rpc_url` | `offline_download_temp_dir` |
| --- | --- | --- |
| AsterDrive 和 aria2 都在 Compose 网络里 | `http://aria2:6800/jsonrpc` | `/data/.tmp/offline-download` |
| AsterDrive 用 `cargo run` 跑在宿主机，aria2 用 Compose 跑在容器里 | `http://127.0.0.1:6800/jsonrpc` | 双方都能看到的同一个宿主机绝对路径 |

`offline_download_aria2_rpc_secret` 填 `ASTERDRIVE_ARIA2_RPC_SECRET` 的值。保存前可以在链接导入引擎注册表里点"测试 aria2"；服务端会用当前草稿调用 `aria2.getVersion` 验证 RPC 地址、密钥和连通性。

:::caution[不要公开 aria2 RPC]
生产环境不要把 aria2 的 `6800` 端口暴露到公网。如果不需要宿主机上的 AsterDrive 访问它，也不要发布到宿主机。
:::

## 常见问题

| 现象 | 常见原因 | 处理方式 |
| --- | --- | --- |
| "测试 aria2"通过，但真实任务失败，日志有 `Permission denied` 或 `Failed to make the directory ...` | RPC 通了，但 aria2 无法写入 AsterDrive 传给它的任务临时目录 | 设置 `offline_download_temp_dir` 为双方都能访问的同一个绝对路径。全 Docker 部署建议 `/data/.tmp/offline-download`；宿主机 `cargo run` + Compose aria2 要把宿主机绝对路径挂进容器的同一个路径 |
| "测试 aria2"认证失败 | `offline_download_aria2_rpc_secret` 和 aria2 的 `RPC_SECRET` 不一致 | 重设 `ASTERDRIVE_ARIA2_RPC_SECRET`，重启 aria2，并在系统设置里保存同一个 secret |
| 敏感输入框显示为空 | 敏感配置读取时会脱敏，前端不会把 `***REDACTED***` 填回输入框 | 留空表示不更改；要修改时直接输入新 secret |
| 连接不上 `http://aria2:6800/jsonrpc` | 只有 AsterDrive 也在 Compose 网络里时，服务名 `aria2` 才能解析 | 全 Docker 用 `http://aria2:6800/jsonrpc`；宿主机运行 AsterDrive 时用 `http://127.0.0.1:6800/jsonrpc`，并确认 Compose 发布了 `6800:6800` |
| aria2 失败后任务仍然成功 | 注册表里 aria2 后面还有 `builtin` 兜底 | 这是预期行为。看任务详情名称、结果和日志确认实际完成的引擎；如果只想暴露 aria2 问题，临时关闭 `builtin` |
| 日志里出现 `Active Download not found for GID...` | 清理阶段发现 aria2 已经没有这个 GID | 这通常不是根因；重点看它前面第一条 aria2 失败日志 |
| 关闭所有引擎后无法创建任务 | 全部关闭表示显式关闭链接导入 | 重新启用至少一个引擎 |

## 相关页面

- [上传与下载：从链接导入文件](/using/upload-download/)
- [系统设置](/reference/config/runtime/)
- [Docker 部署](/deploy/docker/)
- [运维 CLI](/ops/cli/)
