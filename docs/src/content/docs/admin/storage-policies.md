---
description: "AsterDrive 存储策略与策略组的概念权威页：两层模型、首次启动默认状态、策略字段、连接测试、容量观测、迁移预检查与 Blob 匹配规则，以及不要直接改的字段。"
title: "存储策略与策略组"
---

:::tip[两层模型]
- **`管理 -> 存储策略`**：文件真正写到哪里
- **`管理 -> 策略组`**：用户或团队上传时命中哪条存储策略

用户和团队不是直接绑存储策略，而是绑**策略组**；策略组再按规则把上传分到具体策略。
具体怎么接某种后端，看 [存储后端](/admin/storage-backends/) 里的教程。
:::

## 第一次启动后默认会有什么

两种 deployment profile 使用同一套初始化状态机：

| Profile | 首次启动行为 |
| --- | --- |
| `single` | 创建首个管理员后进入 `needs_storage`；管理员可以把 `local` 或其他支持的策略设为默认 |
| `cluster` | 创建首个管理员后同样进入 `needs_storage`；默认策略必须由所有 Primary 访问，不能使用 `local` |

管理员把第一条策略设为默认时，系统会原子创建或协调默认策略组，并回填尚未分配策略组的管理员，随后进入 `ready`。之后创建的新用户会自动绑定当前默认策略组，再由该组决定上传目标。single 和 cluster 调用的是同一套创建、回填和状态迁移代码，区别只在允许选择的存储能力。

系统管理员创建新团队时，如果没有手动指定策略组，会使用当前默认策略组。

## 当前支持的存储类型

<!-- storage-connectors:policy-catalog:start -->
| Connector ID | 后端 | 凭据模式 | 详细教程 |
| --- | --- | --- | --- |
| `asterdrive.storage.local` | 本机 | 无 connector 凭据 | [本机](/admin/storage-backends/local/) |
| `asterdrive.storage.s3` | S3 | 静态密钥 | [S3](/admin/storage-backends/s3/) |
| `asterdrive.storage.alibaba_oss` | 阿里云 OSS | 静态密钥 | [阿里云 OSS](/admin/storage-backends/alibaba-oss/) |
| `asterdrive.storage.sftp` | SFTP | 静态密钥 | [SFTP](/admin/storage-backends/sftp/) |
| `asterdrive.storage.azure_blob` | Azure Blob | 静态密钥 | [Azure Blob](/admin/storage-backends/azure-blob/) |
| `asterdrive.storage.tencent_cos` | 腾讯云 COS | 静态密钥 | [腾讯云 COS](/admin/storage-backends/tencent-cos/) |
| `asterdrive.storage.remote` | 远程节点 | 无 connector 凭据 | [远程节点](/admin/storage-backends/remote-follower/) |
| `asterdrive.storage.onedrive` | OneDrive | 委托 OAuth | [OneDrive](/admin/storage-backends/onedrive/) |
| `asterdrive.storage.qiniu` | 七牛云 Kodo | 静态密钥 | [七牛云 Kodo](/admin/storage-backends/qiniu-kodo/) |
<!-- storage-connectors:policy-catalog:end -->

## 存储策略 vs 策略组

- 只想改"文件最终落到哪种存储后端" —— 创建或编辑存储策略
- 想让不同用户、团队、文件大小走不同路线 —— 配置策略组

后台典型操作顺序：

1. 创建或测试好存储策略
2. 创建策略组规则
3. 把用户或团队绑定到目标策略组

最常见的做法：

- 默认策略组只有一条规则，全部文件都走当前默认策略；单实例可以是本地策略，多 Primary 应使用共享策略
- 同时使用本地和 S3 时，按文件大小拆成多条规则
- 不同用户或团队绑定不同策略组
- 把某个策略组设为新用户默认策略组

策略组可以先禁用，禁用后不能再分配给新用户或团队。如果要删除一个仍被用户或团队绑定的策略组，先用页面里的"迁移绑定关系"把用户和团队绑定批量迁到另一组，再删除。

如果你是在迁移已有数据，不要把旧策略的路径、bucket、endpoint 或远程节点直接改成新位置。先新建目标策略，再用 `管理 -> 存储策略 -> 迁移数据` 创建迁移任务，最后再调整策略组。

## 存储策略的常见字段

| 项目 | 作用 |
| --- | --- |
| 名称 | 后台显示名 |
| 驱动类型 | `local`、`s3`、`alibaba_oss`、`azure_blob`、`tencent_cos`、`one_drive`、`sftp` 或 `remote` |
| 连接信息 | 本地目录 / S3 endpoint、bucket、密钥 / OSS 公网 endpoint、可选服务端 endpoint、region、bucket、CNAME、密钥 / Azure Blob endpoint、container、账号密钥 / COS endpoint、bucket、密钥 / OneDrive Microsoft Graph 目标与授权配置 / SFTP endpoint、SSH 凭据、主机密钥指纹 / 绑定的远程节点 |
| 基础路径 | 写入该策略时使用的目录、prefix 或远程落点相对路径 |
| 单文件大小上限 | 允许上传的最大文件；`0` = 不限 |
| 分片大小 | 大文件上传时每一片的大小 |
| 默认策略 | 新建默认组或默认分流规则会优先使用 |
| 附加选项 | 本地内容去重、S3 / OSS / Azure Blob / COS 上传下载方式、S3 path-style 访问、OSS CNAME、OneDrive 目标 drive 定位、SFTP 主机密钥指纹、远程上传下载方式、存储原生处理开关等 |

后台的存储策略表单不是靠前端硬编码各个厂商字段。AsterDrive 会从后端的 `StorageConnector` descriptor 读取当前 driver 支持的字段、能力、上传工作流和管理动作，所以新增或调整存储后端时，管理界面会尽量跟着后端能力显示。

## 连接测试怎么看

存储策略有两类连接测试：

- **测试已保存策略**：对数据库里已经保存的策略做读写探测。
- **测试草稿配置**：在保存前用当前表单参数做探测；S3、阿里云 OSS、Azure Blob 和 Tencent COS 这类静态凭据后端，在密钥字段留空时可以复用已保存凭据。

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

:::caution[存储原生处理可能产生云厂商费用]
`存储原生处理` 是每条存储策略自己的总开关。开启后，AsterDrive 才会调用当前存储 driver 暴露的原生数据处理能力；在腾讯云 COS 策略下，这对应 COS 数据万象。

AsterDrive 会缓存缩略图和媒体信息等派生结果，避免每次查看文件都重新处理；但首次生成或云厂商侧处理请求仍可能产生费用。腾讯云 COS 的具体配置、后缀策略和免费额度说明见 [腾讯云 COS 存储策略教程](/admin/storage-backends/tencent-cos/)。
:::

## 容量观测与迁移预检查

存储策略编辑弹窗会显示当前容量观测结果：

| 策略类型 | 容量观测行为 |
| --- | --- |
| `local` | 读取策略基础目录所在文件系统的总量、可用量和已用量 |
| `s3` / `alibaba_oss` / `tencent_cos` | 返回"不支持"；这些对象存储 API 没有统一可靠的 bucket 剩余容量接口 |
| `azure_blob` | 返回"不支持"；Blob data API 不提供统一的 storage account 容量观测 |
| `one_drive` | 读取 Microsoft Graph drive quota；如果 Graph 未返回 quota，则显示"不可用" |
| `sftp` | 返回"不支持"；SFTP 协议没有统一可靠的远端文件系统容量接口 |
| `remote` | 通过内部远程存储协议询问策略绑定的远程存储目标；如果目标是 local，通常能看到文件系统容量；如果目标是 S3，则同样显示"不支持" |

迁移数据时，预检查会用目标策略的可用容量和"预计需要复制的 blob 字节数"比较，而不是简单使用源策略总大小。目标策略已经有的 content SHA-256 blob 会被视为可复用，不再计入预计复制量。

容量检查状态含义：

| 状态 | 含义 | 是否阻止创建迁移任务 |
| --- | --- | --- |
| 充足 | 目标可用容量大于或等于预计复制字节数 | 否 |
| 不足 | 目标明确没有足够容量 | 是 |
| 不支持 | 驱动没有可靠容量接口，例如 S3/OSS/COS/Azure Blob | 否，会提示确认容量 |
| 不可用 | 本次容量查询失败或返回信息不完整 | 否，会提示确认容量 |

## 存储迁移中的 Blob 匹配规则

迁移以 blob 为单位处理，不会为每个文件记录重复复制对象。为了避免错误合并，AsterDrive 区分两类 blob key：

| 类型 | 判断方式 | 迁移匹配规则 |
| --- | --- | --- |
| 内容 SHA-256 | 64 位十六进制字符串 | 目标策略已有相同 hash 且 size 相同的 blob 时，会校验目标对象后合并引用 |
| Opaque key | 其他任意 blob key | 不参与跨策略匹配，也不会因为 key 和 size 一样就合并 |

如果 content SHA-256 hash 相同但 size 不同，迁移会失败并保留源 blob 不变。这通常代表数据库或对象存储状态异常，需要管理员检查。

如果 opaque key 在目标策略已经存在，迁移不会覆盖目标对象，也不会把源 blob 合并到目标 blob。系统会为源 blob 生成新的 `migration-...` key，把对象复制到目标策略的新路径，并在任务结果里记录"已重命名 Opaque Key"数量。

## 哪些修改不要直接做

:::caution[已经有文件写入的策略，不要改这些]

- 本地目录
- Bucket
- Endpoint
- Azure container
- OneDrive drive / root item / site / group 定位字段
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

:::caution[迁移不是备份]
迁移任务用于搬迁 AsterDrive 已知的文件对象和引用关系，不替代数据库、配置和对象存储备份。生产迁移前仍然要先看 [备份与恢复](/ops/backup/)。
:::

## 日常维护

- 删除最后一个默认策略或策略组后，系统会回到 `needs_storage`；重新创建并配置默认对象后才能恢复上传
- 已有文件仍按原策略读取；需要搬迁数据时使用存储迁移流程，不要直接改动已有策略的落点
- 保存前先做一次连接测试
- 给不同用户/团队分配不同存储路线时，到 `管理 -> 用户` 或 `管理 -> 团队` 里绑策略组
- 接入外部后端时优先看 [存储后端](/admin/storage-backends/) 里的具体教程
