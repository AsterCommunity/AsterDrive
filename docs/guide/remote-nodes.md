# 远程节点

::: tip 这一篇讲什么
这一篇只讲怎么把另一台 AsterDrive 接成**从节点**，以及主控端怎么登记远程节点、生成 enroll 命令、验证连通性。

如果你连主控实例都还没跑起来，先看 [部署概览](/deployment/)。
:::

::: tip 如果 follower 用 Docker
现在 Docker follower 已经支持在容器启动时直接读取 bootstrap ENV 自动 enroll。  
如果你不想再手动进容器执行 `aster_drive node enroll`，直接看 [Docker 部署从节点](/deployment/docker-follower)。
:::

## 先把概念说清楚

AsterDrive 的远程节点能力，本质上是让**另一台 AsterDrive** 充当存储后端。

- **主控节点**：负责登录、前端、管理后台、分享、WebDAV、存储策略和远程节点管理
- **从节点**：只提供 `/health`、`/health/ready` 和内部远程存储协议；接收主控端签名后的对象请求，再按主控端下发的**接收落点**把对象落到 follower 本地目录或 S3

默认情况下，AsterDrive 跑在 `primary` 模式。  
只有把 `[server].start_mode` 切成 `follower`，它才会变成从节点。

::: warning 这不是多主集群
从节点不是第二个登录站点，也不是第二套管理后台。

它的目标只有一个：**给主控节点提供远程对象存储落点**。  
如果你要的是多主热备、自动故障切换、跨地域强一致复制，当前这套能力还不是那个东西。
:::

## 接入前先想清楚这几件事

### 主控和从节点必须彼此独立

它们可以互相通信，但**不能共用同一套 `data/`、数据库、上传目录或临时目录**。

最起码要独立开的东西包括：

- `data/config.toml`
- 数据库文件，或者外部数据库连接
- 本地上传目录
- 临时目录

### `public_site_url` 是 enroll 的前置条件

主控端生成 enroll 命令时，会直接读取：

```text
管理 -> 系统设置 -> 站点配置 -> 公开站点地址
```

如果这里没填真实可访问的 HTTP(S) 来源，后台就签不出命令。多来源配置时，enroll 命令使用第一行作为主控地址，所以把 follower 能访问到的主控域名放在第一行。

### `base_url` 决定主控能不能主动访问从节点

创建远程节点记录时，`base_url` 可以暂时留空。  
这样你可以先登记、先 enroll，但主控端之后会有两个限制：

- 不能主动测试连通性
- 不能真的把远程存储流量发过去

所以结论很简单：

- **只是先登记**：可以先不填 `base_url`
- **真要承接远程存储策略**：必须填一个主控能访问到的 `http://` 或 `https://` 地址

### 第一次接从节点，先用一个本地接收落点

当前版本里，follower 接收对象的位置由主控端在 `管理 -> 远程节点` 里创建，名字叫**接收落点**。  
第一次接从节点，建议先创建一个 `local` 接收落点，路径就用简单的相对目录，例如：

```text
default
```

这个路径会被 follower 限制在自己的 `server.follower.managed_ingress_local_root` 下面，不会让主控端随便写宿主机上的任意路径。  
原因不是“S3 不能用”，是**先把主从链路跑通，再换复杂落点，排查最省时间**。

## 1. 先把主控节点配好

主控节点就是普通的 `primary` 部署。

开始接从节点前，先确认：

- 主控端后台能正常打开
- `公开站点地址` 已经填好
- 你打算给这个从节点分配的命名空间已经想明白

命名空间别想太复杂。第一次试，按环境、地域或租户拆就够了，例如：

- `home-storage`
- `hangzhou-a`
- `tenant-a`

## 2. 准备从节点实例

从节点和主控节点一样，还是同一个 `aster_drive` 二进制，只是运行模式不同。

最少要确认下面几件事：

- 它有自己的工作目录和数据卷
- 它的 `[server].start_mode` 是 `follower`
- 如果要用主控下发的本地接收落点，`[server.follower].managed_ingress_local_root` 指向容量合适的目录

最直白的写法是改 `config.toml`：

```toml
[server]
start_mode = "follower"

[server.follower]
managed_ingress_local_root = "data/managed-ingress"
```

如果你是 Docker 部署，也可以用环境变量覆盖：

```bash
ASTER__SERVER__START_MODE=follower
```

::: details 当前目录里还没有 `config.toml` 怎么办？
`aster_drive node enroll` 在当前目录还没有配置文件时，会按 follower 模式生成一份默认 `data/config.toml`，并顺手初始化数据库状态。

但你至少要先决定：

- 这个目录是不是以后服务真正运行的工作目录
- 这个目录下面的 `data/` 会不会被持久化

别在临时目录里 enroll 成功了，最后 systemd 或 Docker 实际跑的是另一套数据卷。
:::

## 3. 在主控端登记远程节点

入口：

```text
管理 -> 远程节点
```

创建记录时最关键的是这三项：

- **名称**：给人看的，方便你在后台和策略里识别
- **命名空间**：主控和从节点约定的对象前缀
- **`base_url`**：主控未来访问从节点时使用的地址

保存后，后台会生成一条一次性命令，形态大概像这样：

```bash
aster_drive node enroll --master-url https://drive.example.com --token enr_xxxxx
```

这个 token 默认 **30 分钟** 过期。过期了就回主控端重新生成，不要试图猜它还能不能用。

## 4. 到从服务器执行 enroll

进入从节点自己的工作目录后，执行刚才那条命令。

如果你要显式指定数据库，可以这样追加参数：

```bash
aster_drive node enroll \
  --master-url https://drive.example.com \
  --token enr_xxxxx \
  --database-url sqlite:///data/asterdrive.db?mode=rwc
```

这条命令会做几件事：

- 用 token 去主控端兑换一次性的 bootstrap 配置
- 在从节点本地写入主控绑定和接收命名空间
- 把这次 enroll 回执写回主控端，让主控知道这条接入已经完成

注意，这一步**不会顺手创建接收落点**。  
接收落点现在由主控端在远程节点详情里下发，原因很简单：管理员后续要在同一个地方看到它、改它、测试它，而不是跑去 follower 机器上猜当时 CLI 传了什么。

如果当前配置还是 `primary` 模式，CLI 会直接报错，并要求你先把 `start_mode` 改成 `follower`。  
这不是 bug，是故意拦你，免得你把普通主控实例误接成从节点。

## 5. 重启从节点服务，再回主控测试

当前版本里，enroll 把主控绑定写进数据库后，**运行中的从节点服务不会自动热刷新**。  
所以流程一定是：

1. 执行 `node enroll`
2. 重启从节点服务
3. 回主控端点击“测试连接”

这里有个很容易误判的地方：

| 接口 | enroll 前 | enroll 后 |
| --- | --- | --- |
| `/health` | 返回 `200` 代表进程活着 | 仍然应该返回 `200` |
| `/health/ready` | 返回 `503` 是正常的，因为还没有启用中的主控绑定 | 重启并接入成功后应返回 `200` |

所以别看到 follower 的 `/health/ready` 是 `503` 就以为服务挂了。  
在 enroll 前，它本来就还“不 ready”。

## 6. 在主控端创建接收落点

回到：

```text
管理 -> 远程节点
```

打开刚才这台 follower，找到**主控指定的接收落点**。这里决定主控写到 follower 的对象最后落在哪里。

当前支持两类接收落点：

- `local`：写入 follower 本地目录
- `s3`：写入 follower 能访问的 S3 / MinIO / R2 这类对象存储

第一次建议创建 `local`：

- 名称填一个容易识别的名字，例如 `default-local`
- 基础路径填相对路径，例如 `default`
- 勾选“设为默认接收落点”

这里的本地路径**只能是相对路径**，并且始终会被限制在 follower 的：

```toml
[server.follower]
managed_ingress_local_root = "data/managed-ingress"
```

也就是说，`base_path = "default"` 最终会落到 follower 的 `data/managed-ingress/default` 这一类目录下面。  
如果你想让 follower 直接把对象写到 S3，也是在这里新建 `s3` 接收落点，填 endpoint、bucket、凭证和可选前缀。

::: warning 没有默认接收落点，远程写入会被拒绝
enroll 成功只代表主从身份绑定成功。  
真正接收对象前，follower 还需要一个已应用的默认接收落点。否则远程策略上传时会返回“还没有默认接收落点”。
:::

接收落点由主控端通过 follower API 下发，所以这里还有两个前提：

- 远程节点必须填了主控可访问的 `base_url`
- 当前 follower 只能绑定一个 primary；多 primary 绑定会拒绝这套托管接收落点模式

## 7. 回主控端创建远程存储策略

从节点接入完成后，回到主控端：

```text
管理 -> 存储策略
```

这里可以新建 `远程节点` 类型的存储策略。它和本地 / S3 策略最大的区别是：

- 真正的网络传输、访问密钥和签名都由“远程节点记录”负责
- 策略本身只负责远端路径前缀、上传限制，以及是否设为默认
- 远程存储策略只能绑定**已启用且已配置 `base_url`** 的远程节点
- follower 真正写到哪里，由上一步的默认接收落点决定

也就是说，**别再往远程存储策略里找 endpoint、access key、secret key**；那一层已经被远程节点记录托管掉了。

建好策略之后，再把它放进策略组，或者设成默认路线，后续就和本地 / S3 一样了。

## 常见判断题

### `base_url` 留空能不能 enroll？

能。

但结果只是：

- 主控端保存了节点记录
- 从节点能写入主控绑定

还做不到：

- 主控端主动测试连通性
- 把远程存储策略真正写到这个节点

### 从节点能不能开给普通用户登录？

不能，至少当前设计就不是这么用的。

`follower` 模式只暴露：

- `/health`
- `/health/ready`
- 内部远程存储 API

### 接收落点能不能再选一个 remote 策略？

不能。  
从节点接收入站对象时，落点必须能在 follower 这一侧直接写入，例如 `local` 或 `s3`；不能再套一层 `remote`。

### enroll 成功后为什么还得重启？

因为当前版本只把绑定写进数据库，不会对正在运行的 follower 进程做热刷新。  
**写入成功 ≠ 已经生效**，重启之后才真正开始接流量。

### 禁用远程节点会发生什么？

主控端的远程策略会停止使用它；从节点也会拒绝对应的签名入站请求。  
所以禁用不是“只是在后台里隐藏一下”，而是真的让这条链路停掉。
