# 存储策略

::: tip 这一篇只讲模型和边界
- **`管理 -> 存储策略`**：文件真正写到哪里
- **`管理 -> 策略组`**：用户或团队上传时命中哪条存储策略
- **后端教程**：具体怎么接存储策略后端，看 [存储策略后端](/storage/)

用户和团队不是直接绑存储策略，而是绑**策略组**；策略组再按规则把上传分到具体策略。
:::

## 第一次启动后默认会有什么

新部署实例第一次启动后，系统会自动准备：

- 默认本地存储策略 `Local Default`
- 默认策略组 `Default Policy Group`

什么都不改的话：新用户自动绑默认策略组，再由默认策略组把上传分到默认本地存储策略。系统管理员创建新团队时，没手动指定就用默认策略组。

## 当前支持的存储类型

| 类型 | 说明 | 详细教程 |
| --- | --- | --- |
| `local` | 文件存到本地目录 | [本地磁盘](/storage/local) |
| `s3` | 文件存到 S3 或兼容对象存储（MinIO / R2 / B2 / OSS 等） | [S3 / MinIO / R2](/storage/s3-minio-r2) |
| `azure_blob` | 文件存到 Azure Blob Storage container，使用 Azure Blob SDK 和 SAS URL | [Azure Blob Storage](/storage/azure-blob) |
| `tencent_cos` | 文件存到腾讯云 COS；基础对象读写复用 S3 兼容能力，并额外暴露 COS 数据万象等腾讯云原生能力 | [腾讯云 COS](/storage/tencent-cos) |
| `one_drive` | 文件写到 Microsoft Graph 可访问的 OneDrive、SharePoint 或 Microsoft 365 group drive | [OneDrive](/storage/onedrive) |
| `sftp` | 文件通过 AsterDrive 服务端流式读写到 SSH/SFTP 文件服务器 | [SFTP](/storage/sftp) |
| `remote` | 文件通过内部远程存储协议写到另一台 AsterDrive 从节点 | [远程节点存储策略](/storage/remote-follower) |

## 存储策略 vs 策略组

- 只想改“文件最终落到哪种存储后端” —— 创建或编辑存储策略
- 想让不同用户、团队、文件大小走不同路线 —— 配置策略组

后台典型操作顺序：

1. 创建或测试好存储策略
2. 创建策略组规则
3. 把用户或团队绑定到目标策略组

如果你是在迁移已有数据，不要把旧策略的路径、bucket、endpoint 或远程节点直接改成新位置。先新建目标策略，再用 `管理 -> 存储策略 -> 迁移数据` 创建迁移任务，最后再调整策略组。

## 存储策略的常见字段

| 项目 | 作用 |
| --- | --- |
| 名称 | 后台显示名 |
| 驱动类型 | `local`、`s3`、`azure_blob`、`tencent_cos`、`one_drive`、`sftp` 或 `remote` |
| 连接信息 | 本地目录 / S3 endpoint、bucket、密钥 / Azure Blob endpoint、container、账号密钥 / COS endpoint、bucket、密钥 / OneDrive Microsoft Graph 目标与授权配置 / SFTP endpoint、SSH 凭据、主机密钥指纹 / 绑定的远程节点 |
| 基础路径 | 写入该策略时使用的目录、prefix 或远程落点相对路径 |
| 单文件大小上限 | 允许上传的最大文件；`0` = 不限 |
| 分片大小 | 大文件上传时每一片的大小 |
| 默认策略 | 新建默认组或默认分流规则会优先使用 |
| 附加选项 | 本地内容去重、S3 / Azure Blob / COS 上传下载方式、S3 path-style 访问、OneDrive 目标 drive 定位、SFTP 主机密钥指纹、远程上传下载方式、存储原生处理开关等 |

后台的存储策略表单不是靠前端硬编码各个厂商字段。AsterDrive 会从后端的 `StorageConnector` descriptor 读取当前 driver 支持的字段、能力、上传工作流和管理动作，所以新增或调整存储后端时，管理界面会尽量跟着后端能力显示。

## 连接测试怎么看

存储策略有两类连接测试：

- **测试已保存策略**：对数据库里已经保存的策略做读写探测。
- **测试草稿配置**：在保存前用当前表单参数做探测；S3、Azure Blob 和 Tencent COS 这类静态凭据后端，在密钥字段留空时可以复用已保存凭据。

连接测试成功时只表示 AsterDrive 服务端能访问后端，并且凭据、bucket / container / drive / follower 远程存储目标等基础读写路径可用。它不代表浏览器一定能直连对象存储或 follower。只要用了 `presigned`，还要继续检查浏览器网络、HTTPS 证书、CORS 和暴露响应头。

连接测试失败时，后台会优先展示标准错误响应里的 `error.diagnostic.message`。这个诊断来自后端对存储错误的归类，会尽量保留可排查的信息，同时脱敏 SAS、account key、secret key 等敏感内容。脚本或第三方客户端也应该读：

```json
{
  "code": "storage.permission_denied",
  "msg": "Storage permission denied",
  "error": {
    "retryable": false,
    "diagnostic": {
      "kind": "permission",
      "message": "provider denied access to the target prefix"
    }
  }
}
```

这里的 `code` 仍然是稳定错误码；`diagnostic.message` 是给管理员排查的说明，不要拿它做程序分支。

::: warning 存储原生处理可能产生云厂商费用
`存储原生处理` 是每条存储策略自己的总开关。开启后，AsterDrive 才会调用当前存储 driver 暴露的原生数据处理能力；在腾讯云 COS 策略下，这对应 COS 数据万象。

AsterDrive 会缓存缩略图和媒体信息等派生结果，避免每次查看文件都重新处理；但首次生成或云厂商侧处理请求仍可能产生费用。腾讯云 COS 的具体配置、后缀策略和免费额度说明见 [腾讯云 COS 存储策略教程](/storage/tencent-cos)。
:::

## 存储类型怎么选

### `local`

适合单机、NAS、文件直接落本地磁盘。目录规划、权限、内容去重和测试策略组流程见 [本地磁盘存储策略教程](/storage/local)。

### `s3`

适合文件放到 MinIO、AWS S3 或其他兼容对象存储。

`s3` 表示通用 S3-compatible 后端。它只依赖通用对象存储 API，不假设某个厂商有自己的数据处理能力。如果你要使用腾讯云 COS 的数据万象能力，应选择 `tencent_cos`，不要把 COS 当普通 `s3` 策略配置。

通用 `s3` 策略可以控制 path-style 访问。开启后请求更接近 `endpoint/bucket/key`，常见于 MinIO、RustFS 等兼容服务；关闭后使用虚拟托管风格，常见于 AWS S3 这类服务。不同厂商和自建网关要求不同，创建或编辑策略后先用连接测试确认。

如果早期已经把腾讯云 COS 配成了通用 `s3` 策略，后台可能会提示把 driver 提升为 `tencent_cos`。这个操作不迁移对象，也不改 bucket；它只是让这条策略改用腾讯云 COS driver。系统只允许明确白名单内的提升方向，并会拒绝活动上传会话或 bucket 不一致的情况。

配置 bucket、凭证、CORS、上传下载方式和策略组分流时，看 [S3 / MinIO / R2 存储策略教程](/storage/s3-minio-r2)。

### `azure_blob`

适合文件放到 Azure Blob Storage container。`azure_blob` 使用 Azure 官方 Blob SDK 和 Azure SAS URL，不走 S3-compatible 接口。

配置时要区分这些字段：Endpoint 是 Blob service endpoint，Bucket 字段对应 Azure container，Access Key 对应 storage account name，Secret Key 对应 storage account key。如果使用 `presigned` 直传，还要配置 Blob service CORS，并允许 `x-ms-blob-type` 请求头。完整流程见 [Azure Blob Storage 存储策略教程](/storage/azure-blob)。

### `one_drive`

适合把文件写到 Microsoft Graph 可访问的 OneDrive、SharePoint 文档库或 Microsoft 365 group drive。

OneDrive 策略需要 Microsoft 应用注册和管理员 delegated OAuth 授权。先保存策略和 Microsoft Graph 应用凭据，再发起授权；授权请求不会携带未保存的 Client ID / Secret 草稿。目标 drive 可以在授权后自动解析，也可以通过 Drive ID、SharePoint site ID 或 group ID 指定。

上传方式可以选择 `server_relay` 或 `frontend_direct`。`server_relay` 是为了兼容已有策略而保留的默认值，文件会经过 AsterDrive；`frontend_direct` 让浏览器直接上传到 Microsoft Graph，可以减少 AsterDrive 节点带宽。直传所需的跨域支持由 Microsoft 提供，不需要在 AsterDrive 中额外配置。完整流程见 [OneDrive 存储策略教程](/storage/onedrive)。

### `sftp`

适合把文件写到 SSH/SFTP 文件服务器、NAS 或传统服务器目录。

SFTP 策略使用服务端流式上传和下载，浏览器不会直接连接 SFTP 服务器。Endpoint 可以写成 `sftp://host:port`、`host` 或 `host:port`；没有端口时默认 `22`，远程根目录放在基础路径里。SSH 用户名 / 密码在 API 字段里仍复用 `access_key` / `secret_key`，但后台表单会显示为 SSH 凭据。

SFTP 默认拒绝未知主机密钥。第一次连接测试会返回服务器实际 `SHA256:...` 指纹，管理员确认后填入 `storage_policy.options.sftp_host_key_fingerprint`，后续连接必须匹配该指纹。完整流程见 [SFTP 存储策略教程](/storage/sftp)。

### `tencent_cos`

适合文件放到腾讯云 COS，并且希望按策略启用腾讯云原生能力。

`tencent_cos` 的基础对象读写、分片上传和下载路径复用 S3-compatible 逻辑；COS 专属部分负责腾讯云 endpoint 规范化、COS 签名以及数据万象能力。完整配置流程见 [腾讯云 COS 存储策略教程](/storage/tencent-cos)。

### `remote`

适合把控制面留在主控节点，把真实对象落点拆到另一台 AsterDrive 从节点。

远程策略绑定远程节点和该节点上的远程存储目标，不再单独填 endpoint 或 access key；没有显式选择目标时，会使用远程节点详情里的**默认远程存储目标**。完整配置流程见 [远程节点存储策略教程](/storage/remote-follower)。

## 容量观测与迁移预检查

存储策略编辑弹窗会显示当前容量观测结果：

| 策略类型 | 容量观测行为 |
| --- | --- |
| `local` | 读取策略基础目录所在文件系统的总量、可用量和已用量 |
| `s3` / `tencent_cos` | 返回“不支持”；标准 S3 兼容 API 没有统一可靠的 bucket 剩余容量接口 |
| `azure_blob` | 返回“不支持”；Blob data API 不提供统一的 storage account 容量观测 |
| `one_drive` | 读取 Microsoft Graph drive quota；如果 Graph 未返回 quota，则显示“不可用” |
| `sftp` | 返回“不支持”；SFTP 协议没有统一可靠的远端文件系统容量接口 |
| `remote` | 通过内部远程存储协议询问策略绑定的远程存储目标；如果目标是 local，通常能看到文件系统容量；如果目标是 S3，则同样显示“不支持” |

迁移数据时，预检查会用目标策略的可用容量和“预计需要复制的 blob 字节数”比较，而不是简单使用源策略总大小。目标策略已经有的 content SHA-256 blob 会被视为可复用，不再计入预计复制量。

容量检查状态含义：

| 状态 | 含义 | 是否阻止创建迁移任务 |
| --- | --- | --- |
| 充足 | 目标可用容量大于或等于预计复制字节数 | 否 |
| 不足 | 目标明确没有足够容量 | 是 |
| 不支持 | 驱动没有可靠容量接口，例如 S3/COS/Azure Blob | 否，会提示确认容量 |
| 不可用 | 本次容量查询失败或返回信息不完整 | 否，会提示确认容量 |

## 存储迁移中的 Blob 匹配规则

迁移以 blob 为单位处理，不会为每个文件记录重复复制对象。为了避免错误合并，AsterDrive 区分两类 blob key：

| 类型 | 判断方式 | 迁移匹配规则 |
| --- | --- | --- |
| 内容 SHA-256 | 64 位十六进制字符串 | 目标策略已有相同 hash 且 size 相同的 blob 时，会校验目标对象后合并引用 |
| Opaque key | 其他任意 blob key | 不参与跨策略匹配，也不会因为 key 和 size 一样就合并 |

如果 content SHA-256 hash 相同但 size 不同，迁移会失败并保留源 blob 不变。这通常代表数据库或对象存储状态异常，需要管理员检查。

如果 opaque key 在目标策略已经存在，迁移不会覆盖目标对象，也不会把源 blob 合并到目标 blob。系统会为源 blob 生成新的 `migration-...` key，把对象复制到目标策略的新路径，并在任务结果里记录“已重命名 Opaque Key”数量。

## 哪些修改不要直接做

::: warning 已经有文件写入的策略，不要改这些

- 本地目录
- Bucket
- Endpoint
- SFTP 基础路径
- 绑定的远程节点

旧文件按原位置读取，直接改位置 = 已有文件全部找不到。

更稳的做法：

1. 新建一条策略
2. 在 `管理 -> 存储策略 -> 迁移数据` 里选择源策略和目标策略
3. 先点 `检查计划`，确认目标探测、流式上传能力和容量检查没有阻塞项
4. 创建迁移任务，并在 `管理 -> 任务` 里确认完成
5. 把用户或团队切到新策略所在的策略组

:::

## 迁移已有策略数据

`迁移数据` 会创建一个后台任务，把源策略下已有 Blob 复制到目标策略，并在迁移过程中更新文件记录和版本引用。

创建任务前，页面会先做一轮 `检查计划`：

- 统计源策略下有多少对象和总大小
- 探测目标策略是否能写入
- 检查目标是否支持迁移需要的流式上传
- 估算目标侧已经存在多少可复用对象，并据此计算实际还需要复制的字节数
- 尽量确认目标剩余容量是否足够承载这部分待复制数据
- 统计 opaque key 冲突数量

只有目标明确容量不足时，预检查才会阻止创建迁移任务。如果容量检查显示不支持或不可用，不等于一定不能迁移；只是当前驱动无法可靠读出剩余空间。正式创建任务前，你需要自己确认目标存储容量够用。

迁移任务创建后，到 `管理 -> 任务` 查看进度。大型迁移建议安排维护窗口，迁移期间尽量避免继续往源策略写入大量新文件。

::: warning 迁移不是备份
迁移任务用于搬迁 AsterDrive 已知的文件对象和引用关系，不替代数据库、配置和对象存储备份。生产迁移前仍然要先看 [备份与恢复](/deployment/backup)。
:::

## 日常维护

- 至少保留一条可用的默认存储策略
- 至少保留一个启用中的默认策略组
- 保存前先做一次连接测试
- 给不同用户/团队分配不同存储路线时，到 `管理 -> 用户` 或 `管理 -> 团队` 里绑策略组
- 接入外部后端时优先看 [存储策略后端](/storage/) 里的具体教程
