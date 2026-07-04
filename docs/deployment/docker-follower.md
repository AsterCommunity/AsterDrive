# Docker 部署从节点

::: tip 这一篇适合谁
想把另一台 AsterDrive 用 Docker 跑成 `follower`，并且希望**首次启动时直接靠环境变量自动 enroll**，不再手动 `docker exec ... aster_drive node enroll`。
:::

这一篇只讲 Docker 场景下的从节点部署。
主控节点怎么理解远程节点、怎么创建远程存储策略，仍然看 [远程节点](/guide/remote-nodes)。

和旧流程最大的区别只有一件事：

- **现在 follower 容器可以在启动时直接读取 bootstrap ENV，自动完成 enroll**

也就是说，第一次启动时不再需要：

- 手动进入容器执行 `aster_drive node enroll`
- enroll 完之后再额外重启一次容器

## 先确认这 4 件事

### 1. 主控节点已经正常运行

最少要满足：

- 主控后台可以正常打开
- `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 已经填成真实可访问的 HTTP(S) 来源；多来源配置时，follower 能访问到的主控来源应放第一行
- 你已经想好这个 follower 的名称，以及准备用直连、反向通道还是自动传输

### 2. follower 必须有自己独立的 `data/`

主控和 follower **绝对不能共用**：

- `data/config.toml`
- 数据库
- 上传目录
- 临时目录

从节点不是“主控的另一个副本”，它是另一台独立的 AsterDrive。

### 3. 先决定直连还是反向通道

如果你选择**直连**，主控必须能访问 follower 的 `base_url`。不同机器上通常会是：

- `https://follower.example.com`
- `http://10.0.0.23:3000`
- `http://host.example.com:3001`

如果它们都在 Docker 网络里，主控也能解析容器名，那么也可以直接填容器内地址。
不要默认填写 `http://localhost:3000`，那通常只对 follower 自己成立。

::: tip 如果 follower 在 NAT 或 CGNAT 后方
如果主控访问不到 follower，但 follower 能访问主控的 `公开站点地址`，可以在创建远程节点时选择**反向通道**或 `auto + 空 base_url`。

反向通道仍处于测试阶段，适合 `relay_stream` 上传/下载；如果你要使用远程 `presigned` 上传或下载，仍然需要直连，并且浏览器也要能访问 follower 的 `base_url`。
:::

### 4. token 是一次性的

主控后台生成的 enrollment token：

- 默认 30 分钟过期
- 成功兑换一次后就作废

所以这类 ENV 只适合**首启 bootstrap**，不要长期把旧 token 留在 Compose 里。

## 1. 在主控后台创建远程节点并生成 token

入口：

```text
管理 -> 远程节点
```

先创建一条远程节点记录，至少填好：

- 名称
- 传输方式
- `base_url`：直连必填；反向通道可以留空；自动模式下留空即走反向通道

保存后，后台会生成一组 enroll 信息。Docker follower 启动时真正需要的是这两个值：

- `master_url`
- `token`

当前版本里，follower 接收对象的落点由主控节点在远程节点详情里下发。
也就是说，Docker bootstrap 只负责完成主从身份绑定；真正写到 follower 本地目录还是 S3，后面回主控后台创建**接收落点**。

## 2. 准备 follower 的数据目录

如果你用 bind mount，把宿主机目录先建好并改属主：

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data
```

如果你用 named volume，可以跳过这一步。

## 3. 写 `compose.yaml`

下面这份示例假设 follower 对外暴露在宿主机 `3001` 端口：

```yaml
services:
  asterdrive-follower:
    image: ghcr.io/astercommunity/asterdrive:latest
    container_name: asterdrive-follower
    ports:
      - "3001:3000"
    environment:
      ASTER__SERVER__HOST: 0.0.0.0
      ASTER__SERVER__START_MODE: follower
      ASTER__SERVER__FOLLOWER__REMOTE_STORAGE_TARGET_LOCAL_ROOT: /data/remote-storage-targets
      ASTER__DATABASE__URL: sqlite:///data/asterdrive.db?mode=rwc
      ASTER_BOOTSTRAP_REMOTE_MASTER_URL: https://drive.example.com
      ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN: enr_replace_me
    volumes:
      - ./data:/data
      - /etc/localtime:/etc/localtime:ro
    restart: unless-stopped
```

这里最容易混在一起的是两类环境变量：

- `ASTER__...`
  这是**长期运行配置覆盖**，和 `config.toml` 同一套结构，建议保留需要长期生效的项
- `ASTER_BOOTSTRAP_REMOTE_*`
  这是**一次性 bootstrap 输入**，首次 enroll 成功后建议移除

这页用到的 ENV 可以按下面理解：

| 环境变量 | 作用 | 建议 |
| --- | --- | --- |
| `ASTER__SERVER__HOST` | 让容器内服务监听所有网卡，方便 Docker 端口映射 | Docker 场景通常保留 |
| `ASTER__SERVER__START_MODE` | 把实例切成 `follower` 模式 | 从节点长期保留 |
| `ASTER__SERVER__FOLLOWER__REMOTE_STORAGE_TARGET_LOCAL_ROOT` | 限制主控下发的 `local` 接收落点根目录 | 需要本地接收落点时保留 |
| `ASTER__DATABASE__URL` | 指定 follower 自己的数据库 | Docker 场景建议显式写清楚 |
| `ASTER_BOOTSTRAP_REMOTE_MASTER_URL` | 首次 enroll 时访问的主控地址 | 成功后移除 |
| `ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN` | 主控生成的一次性 enrollment token | 成功后移除 |

接收落点不放在 bootstrap ENV 里。
现在的做法是：follower 先完成 enroll，主控节点再通过 follower API 下发接收落点。这个入口在 `管理 -> 远程节点`，更适合后续查看、修改和排错。

## 4. 首次启动

```bash
docker compose up -d
docker logs -f asterdrive-follower
```

正常情况下，首次启动会依次完成这些事：

1. 在 `/data/config.toml` 不存在时自动生成配置
2. 以 `follower` 模式启动
3. 用 `ASTER_BOOTSTRAP_REMOTE_MASTER_URL` 和 `ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN` 去主控兑换 bootstrap 信息
4. 在本地数据库写入主控绑定
5. 继续完成 follower 运行时初始化

你应该能在日志里看到类似信息：

- `Configuration loaded from: /data/config.toml`
- `bootstrapped follower enrollment from environment`
- `startup complete — listening on 0.0.0.0:3000`

这条路径里，**不需要再手动执行 `node enroll`，也不需要首启后再额外重启一遍**。

::: tip bootstrap ENV 要成对出现
`ASTER_BOOTSTRAP_REMOTE_MASTER_URL` 和 `ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN` 必须一起设置。

如果只设置其中一个，服务会在启动早期提示配置不完整；如果两个都不设置，就按普通 follower 启动，不会自动 enroll。
:::

## 5. 验证 follower 已经 ready

先看容器状态：

```bash
docker ps
```

再直接检查健康状态：

```bash
curl http://127.0.0.1:3001/health
curl http://127.0.0.1:3001/health/ready
```

期望结果：

- `/health` 返回 `200`
- `/health/ready` 在 enroll 成功并完成启动后也应返回 `200`

然后回主控后台：

```text
管理 -> 远程节点
```

点击“测试连接”。直连节点会访问 `base_url`；反向通道节点会通过 follower 主动建立的通道访问，刚启动时可能需要等几十秒让通道变成在线。测试通过后，先打开这台 follower 的远程节点详情，创建一个**默认接收落点**。

测试连接通过时，主控也会读取 follower 的内部存储协议能力。当前内部协议版本是 `v4`，并要求 follower 支持 `v4`；如果能力摘要显示协议不兼容，先升级主控或 follower，别急着创建 remote 策略。

第一次建议选：

- 驱动：`local`
- 基础路径：`default` 这类相对路径
- 勾选“设为默认接收落点”

local 接收落点的路径会被限制在 follower 的 `server.follower.remote_storage_target_local_root` 下面。
如果你要让 follower 再写到 S3 / MinIO，也是在这里创建 `s3` 接收落点，而不是在 bootstrap ENV 里传。

接收落点应用成功后，再去：

```text
管理 -> 存储策略
```

创建 `远程节点` 类型的存储策略。

如果远程策略选择 `presigned` 上传或下载，还要确认远程节点使用直连传输、浏览器能访问 follower 的 `base_url`，并且 follower 前面的反向代理没有吞掉内部存储 API 的 CORS 头。上传需要允许 `content-type` 并暴露 `ETag`；下载 Range 需要允许 `range` 并暴露 `Accept-Ranges`、`Content-Range`、`Content-Length`。

## 6. 首次成功后，把一次性 bootstrap ENV 移掉

确认 follower 已经 ready、主控测试连接通过、默认接收落点也已应用后，把这几个 ENV 从 Compose 里删掉：

- `ASTER_BOOTSTRAP_REMOTE_MASTER_URL`
- `ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN`

然后重新执行：

```bash
docker compose up -d
```

数据库里的主控绑定已经持久化了；后续重启 follower，不需要再重复 bootstrap。
但 `ASTER__SERVER__START_MODE=follower` 这种长期运行配置，仍然应该保留。

## 常见问题

### 日志里提示 token 已完成、已过期或已被替换

这说明你拿的是旧 token。
回主控后台重新生成一条新的 enrollment token，再更新 Compose。

### `/health` 是 200，但 `/health/ready` 还是 503

通常表示 follower 进程活着，但主控绑定还没有生效。优先检查：

- bootstrap ENV 有没有写对
- token 有没有过期
- follower 本地数据库里是否真的写入了绑定
- 日志里是否出现 bootstrap 失败 warning

### follower 能启动，但主控测试连接失败

优先检查这些事：

- 远程节点传输方式是不是选对了
- 直连模式下，主控后台里填的 `base_url` 是不是主控真正能访问到的地址
- 反向通道模式下，follower 能不能访问主控的 `公开站点地址`，代理或防火墙有没有拦 WebSocket / 长连接
- 直连模式下，端口映射、反向代理或 NAT 有没有把流量正确转到 follower 的 `3000`
- follower 的 `server.host` 是否允许外部访问

### 已有旧的 `/data/config.toml`，里面还是 `primary`

最稳的做法有两个：

- 直接在 `/data/config.toml` 里把 `[server].start_mode` 改成 `follower`
- 或者像上面的 Compose 一样，长期保留 `ASTER__SERVER__START_MODE=follower`

bootstrap token 不会自动把既有 `primary` 配置改为 `follower`。
请显式修改 `start_mode`，或长期保留环境变量覆盖。

如果启动时已经带了 `ASTER_BOOTSTRAP_REMOTE_*`，但最终加载出来的模式仍然是 `primary`，服务会停止并提示先切到 `follower`，这是为了避免把主控实例误接成从节点。
