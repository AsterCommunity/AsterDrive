# 服务器配置

::: tip 这一篇覆盖 `[server]` 这一组
监听地址、端口、工作线程数、临时目录、节点启动模式——决定服务"对外露在哪、临时文件落在哪、作为 primary 还是 follower 跑"。
大多数 primary 部署只需要确认两件事：`host` 是不是 `0.0.0.0`、临时目录是不是在容量充足的盘上。
:::

```toml
[server]
host = "127.0.0.1"
port = 3000
workers = 0
temp_dir = ".tmp"
upload_temp_dir = ".uploads"
start_mode = "primary"

[server.follower]
remote_storage_target_local_root = "remote-storage-targets"
```

如果 `data/config.toml` 是自动生成的，运行时实际会把相对路径解析到 `data/` 下，例如 `data/.tmp`、`data/.uploads` 和 `data/remote-storage-targets`。

## 什么时候需要改

- **容器/Docker 部署** —— `host` 改成 `0.0.0.0`，否则容器外打不进来
- **端口被占用** —— 改 `port`
- **临时目录所在盘容量小** —— 把 `temp_dir` 和 `upload_temp_dir` 挪到大盘
- **不确定线程数** —— 保持 `workers = 0` 让它按 CPU 自动决定
- **这台实例要作为从节点** —— 把 `start_mode` 改成 `follower`，并确认 `remote_storage_target_local_root` 落在容量合适的盘上

## 选项一览

| 选项 | 默认值 | 作用 |
| --- | --- | --- |
| `host` | `"127.0.0.1"` | 监听地址；容器部署改成 `0.0.0.0` |
| `port` | `3000` | HTTP 监听端口 |
| `workers` | `0` | 工作线程数；`0` = 按 CPU 自动 |
| `temp_dir` | `".tmp"` | 服务端通用临时文件目录 |
| `upload_temp_dir` | `".uploads"` | 分片上传 / 上传恢复用的临时目录 |
| `start_mode` | `"primary"` | 节点启动角色；`primary` 是普通主控，`follower` 是远程存储从节点 |
| `follower.remote_storage_target_local_root` | `"remote-storage-targets"` | follower 受主控托管的 local 接收落点根目录 |

## 临时目录会用在哪

`temp_dir` 和 `upload_temp_dir` 直接影响本地磁盘占用，主要消耗在：

- 大文件分片上传
- 上传恢复（断点续传）
- 本地存储的临时拼装
- 少数需要服务端临时处理的上传路径

::: tip 经常上传大文件就挪一下
默认会落到 `data/.tmp` 和 `data/.uploads`。如果你预计大量大文件上传，把这两个目录绑到容量更充足的本地盘。
:::

## `start_mode` 怎么选

默认值是 `primary`。普通部署、登录入口、管理后台、分享、WebDAV、用户文件浏览器，都是 primary 做的事。

只有你明确要把这台机器接成远程存储从节点时，才改成：

```toml
[server]
start_mode = "follower"
```

`start_mode` 是静态启动角色，改完要重启进程。  
follower 不是第二个登录站点，它只提供健康检查和内部远程存储接口。完整接入流程看 [远程节点](/guide/remote-nodes)。

## follower 接收根目录

`[server.follower].remote_storage_target_local_root` 只在 follower 模式下有意义。

主控节点在远程节点详情里创建 `local` 接收落点时，只能填相对路径。follower 会把这个相对路径拼到 `remote_storage_target_local_root` 下面，避免主控节点直接写宿主机任意目录。

例如：

```toml
[server.follower]
remote_storage_target_local_root = "/data/remote-storage-targets"
```

主控节点创建接收落点时填：

```text
base_path = "default"
```

最终写入位置就是：

```text
/data/remote-storage-targets/default
```

这个目录要和真实文件容量一起规划。它不是临时目录，里面会放 follower 接收到的正式对象。

::: tip 配置键在 `[server.follower]` 下面
接收根目录现在是 `server.follower.remote_storage_target_local_root`。

如果旧配置在 `[server.follower]` 下还写着 `managed_ingress_local_root`，仍会被兼容读取；新配置建议改成 `remote_storage_target_local_root`。
:::

## 常见写法

### 本机测试

```toml
[server]
host = "127.0.0.1"
port = 3000
workers = 0
temp_dir = "data/.tmp"
upload_temp_dir = "data/.uploads"
start_mode = "primary"
```

### Docker / 容器

```toml
[server]
host = "0.0.0.0"
port = 3000
workers = 0
temp_dir = "/data/.tmp"
upload_temp_dir = "/data/.uploads"
start_mode = "primary"
```

### Docker follower

```toml
[server]
host = "0.0.0.0"
port = 3000
workers = 0
temp_dir = "/data/.tmp"
upload_temp_dir = "/data/.uploads"
start_mode = "follower"

[server.follower]
remote_storage_target_local_root = "/data/remote-storage-targets"
```

## 几条经验

- 大多数部署不需要手调 `workers`
- 长期部署，临时目录写绝对路径
- 前面已经有反向代理时，应用本身继续监听内部端口即可，不要直接暴露到公网

## 对应环境变量

```bash
ASTER__SERVER__HOST=0.0.0.0
ASTER__SERVER__PORT=3000
ASTER__SERVER__WORKERS=0
ASTER__SERVER__TEMP_DIR=/data/.tmp
ASTER__SERVER__UPLOAD_TEMP_DIR=/data/.uploads
ASTER__SERVER__START_MODE=follower
ASTER__SERVER__FOLLOWER__REMOTE_STORAGE_TARGET_LOCAL_ROOT=/data/remote-storage-targets
```
