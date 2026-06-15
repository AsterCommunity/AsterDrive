---
description: OneDrive 存储策略教程，覆盖 Microsoft 应用注册、Global / China 云端点、delegated permissions、OAuth 授权、目标 drive 解析和应用配置存储设计。
---

# OneDrive 存储策略教程

::: tip 这一篇覆盖什么
这一篇按完整流程讲怎么把 AsterDrive 文件写到 Microsoft OneDrive 或 SharePoint / Microsoft 365 group drive：准备 Microsoft 应用、创建 OneDrive 存储策略、完成 Microsoft Graph 授权、配置策略组规则、绑定用户或团队，并说明当前 Client ID / Secret 的存储设计。
:::

## 适合什么时候用

OneDrive 存储策略适合这些场景：

- 你已经使用 Microsoft 365、OneDrive 或 SharePoint 文档库
- 希望把团队文件写入 Microsoft Graph 可访问的 drive
- 希望通过管理员授权把某个 OneDrive / SharePoint drive 作为 AsterDrive 的后端
- 希望使用 Microsoft Graph delegated permissions，由管理员在浏览器里完成授权

如果你只需要一个通用对象存储后端，S3 / MinIO / R2 或腾讯云 COS 会更直接。OneDrive 的优势是接入 Microsoft 生态，代价是需要正确配置 Microsoft 应用注册、OAuth redirect URI 和 delegated permissions。

## 先分清你要配哪几层

```mermaid
flowchart TD
  MicrosoftApp["Microsoft 应用注册"] --> Policy["AsterDrive OneDrive 存储策略"]
  Policy --> OAuth["Microsoft Graph OAuth 授权"]
  OAuth --> Drive["目标 drive / root item"]
  Drive --> Rule["策略组规则"]
  Rule --> Binding["用户或团队绑定策略组"]
```

只创建 OneDrive 存储策略还不够。策略保存后，还需要在 AsterDrive 后台发起 Microsoft 授权，让 AsterDrive 获得访问目标 drive 的 delegated token。

## 这篇用到的入口

| 你要做什么 | 入口 |
| --- | --- |
| 创建 OneDrive 策略 | `管理 -> 存储策略 -> 新建策略` |
| 复制 Microsoft redirect URI | `管理 -> 存储策略 -> OneDrive 策略 -> Microsoft Graph 凭据` |
| 发起授权或重新授权 | `管理 -> 存储策略 -> OneDrive 策略 -> 授权` |
| 验证已保存凭据 | `管理 -> 存储策略 -> OneDrive 策略 -> 验证` |
| 创建分流规则 | `管理 -> 策略组` |
| 给用户绑定策略组 | `管理 -> 用户 -> 用户详情` |
| 给团队绑定策略组 | `管理 -> 团队 -> 团队详情` |

## 1. 选择 Microsoft 云端点

创建 OneDrive 策略时先选 Microsoft 云端点：

| 云端点 | 登录端点 | Graph 端点 | 适合账号 |
| --- | --- | --- | --- |
| 国际版 | `login.microsoftonline.com` | `graph.microsoft.com` | 个人 Microsoft 账号、Entra ID 工作或学校账号 |
| 中国版（世纪互联） | `login.chinacloudapi.cn` | `microsoftgraph.chinacloudapi.cn` | 中国云组织账号 |

::: warning 不要混用 Global 和 China
Microsoft 应用注册、登录端点和 Graph 端点需要在同一个云环境里。个人 Microsoft 账号不支持中国版端点，如果要使用个人 OneDrive，请选择国际版。
:::

## 2. 准备 Microsoft 应用注册

在 Microsoft Entra ID 应用注册里准备一个应用。

最少需要关注：

- Application (client) ID
- Client Secret，公共客户端场景可以不填，但服务端部署通常建议准备 secret
- Redirect URI
- Microsoft Graph delegated permissions

### Redirect URI 必须完全一致

AsterDrive 会在 OneDrive 策略编辑页显示 redirect URI。把这个地址完整复制到 Microsoft 应用注册里。

常见格式类似：

```text
https://drive.example.com/api/v1/admin/policies/storage-authorization/callback
```

Microsoft 对 redirect URI 做精确匹配。协议、域名、端口、路径只要有一个字符不一致，授权回调就会失败。

### 使用 delegated permissions

OneDrive 存储策略使用管理员在浏览器里完成的 Microsoft Graph delegated authorization，不是 application permissions。

AsterDrive 会按目标类型选择默认授权范围：

| 目标类型 | 默认 scopes |
| --- | --- |
| 个人 OneDrive / 工作或学校默认 OneDrive | `offline_access Files.ReadWrite` |
| 个人或工作学校账号，但显式填写 Drive ID | `offline_access Files.ReadWrite.All` |
| SharePoint site drive / Microsoft 365 group drive | `offline_access Files.ReadWrite.All Sites.ReadWrite.All` |

不要在 AsterDrive 前端手工填写 scopes。管理员只需要在 Microsoft 应用注册中确保这些 delegated permissions 已允许，并在授权时按 Microsoft 页面提示同意。

::: tip 为什么需要 offline_access
`offline_access` 用于获取 refresh token。没有 refresh token，后台缩略图、容量检查和读写任务在 access token 过期后会要求重新授权。
:::

## 3. 创建 OneDrive 存储策略

进入：

```text
管理 -> 存储策略 -> 新建策略
```

选择驱动类型：

```text
OneDrive
```

填写：

| 字段 | 建议 |
| --- | --- |
| Microsoft 云端点 | 按账号所在云选择国际版或中国版 |
| Client ID | Microsoft 应用注册里的 Application (client) ID |
| Client Secret | Microsoft 应用 secret；如果使用公共客户端可留空 |
| Drive 类型 | 新建时通常保持默认，授权后自动解析默认 drive |

保存策略后，进入策略编辑页发起授权。

## 4. 完成 Microsoft 授权

进入 OneDrive 策略编辑页：

```text
管理 -> 存储策略 -> 目标 OneDrive 策略
```

在 `Microsoft Graph 凭据` 区域点击 `授权`。

授权成功后，浏览器会回到 AsterDrive 管理后台，并显示授权结果。AsterDrive 会保存：

- access token ciphertext
- refresh token ciphertext
- token 过期时间
- 授权时间
- 目标 drive / root item metadata
- Microsoft cloud / tenant / app metadata

后续后台任务需要访问 OneDrive 时，会自动刷新 access token。刷新成功会写回数据库；如果 Microsoft 拒绝 refresh token，策略会进入需要重新授权状态。

## 5. 目标 drive 如何解析

默认情况下不需要填写 Drive ID。AsterDrive 会在授权完成后自动解析：

| Drive 类型 | 自动解析方式 |
| --- | --- |
| 个人 OneDrive | 当前登录账号的默认 drive |
| 工作或学校 OneDrive | 当前登录账号的默认 drive |
| SharePoint site drive | 按 Site ID 解析站点默认 drive，除非已填写 Drive ID |
| Microsoft 365 group drive | 按 Group ID 解析 group drive，除非已填写 Drive ID |

高级字段只在你明确需要非默认文档库或固定 root item 时再填写：

| 字段 | 什么时候填 |
| --- | --- |
| Drive ID | 要访问非默认 drive，或者要绕过自动解析 |
| Root item ID | 要把 AsterDrive 写入限定到某个文件夹 |
| Site ID | SharePoint site drive 模式需要，除非已有 Drive ID |
| Group ID | Microsoft 365 group drive 模式需要，除非已有 Drive ID |

::: tip root item
Root item ID 留空或填写 `root` 表示写入 drive 根目录。
:::

## 6. 创建测试策略组

不要一上来直接把真实用户切到新的 OneDrive 策略。建议先创建测试策略组。

进入：

```text
管理 -> 策略组
```

创建策略组，例如：

```text
OneDrive Test Group
```

添加一条规则：

| 字段 | 建议 |
| --- | --- |
| 存储策略 | 刚创建并授权成功的 OneDrive 策略 |
| 优先级 | 保持默认或设为最先命中 |
| 文件大小范围 | 先覆盖所有大小，方便测试 |

## 7. 绑定测试用户或测试团队

### 绑定用户

进入：

```text
管理 -> 用户 -> 用户详情
```

把测试用户的策略组改成刚才创建的 `OneDrive Test Group`。

### 绑定团队

进入：

```text
管理 -> 团队 -> 团队详情
```

把测试团队的策略组改成 `OneDrive Test Group`。

团队空间上传时会按团队策略组走，不按个人用户策略组走。

## 8. 做一轮真实验收

用测试账号至少跑一遍：

1. 上传小文件
2. 上传较大的文件
3. 下载文件
4. 预览图片或触发缩略图生成
5. 删除和恢复文件
6. 在 Microsoft 侧确认对象写入目标 drive
7. 在 AsterDrive 后台点击 `验证`

如果后台任务报 Microsoft Graph `401` 或 token 相关错误，先回到策略编辑页查看凭据状态。状态为需要重新授权时，点击 `重新授权`。

## 9. 当前应用配置存储设计

AsterDrive 当前没有为 OneDrive 单独创建共享应用配置表。Microsoft 应用配置沿用存储策略连接字段：

| 存储字段 | OneDrive 含义 |
| --- | --- |
| `storage_policies.access_key` | Microsoft Application (client) ID |
| `storage_policies.secret_key` | Microsoft Client Secret |

这是刻意的设计折中。AsterDrive 现在的存储策略模型是“每条策略自带连接配置”，S3、COS、Azure Blob 等后端也遵循这个形态。OneDrive 在当前产品需求里不需要多个策略共享同一个 Microsoft App，所以没有提前引入 `oauth_app_configs` 或 `storage_provider_app_configs` 这类表。

这样做的好处是：

- 和现有存储策略连接模型一致
- 创建和授权流程更直接
- 不需要额外处理共享 app config 的权限、引用关系和生命周期
- 不会为了尚未确认的复用需求增加表结构复杂度

如果以后出现明确需求，例如多个 OneDrive 策略共享一个 Microsoft App、Google Drive 也接入 OAuth 存储后端、管理员需要统一轮换 app secret，再考虑迁移到类似下面的模型：

```text
storage_provider_app_configs
storage_policies.app_config_id -> storage_provider_app_configs.id
```

在那之前，继续把 Client ID / Secret 作为 OneDrive 存储策略的连接配置，是更简单也更符合当前架构的做法。

## 常见问题

### 授权回来显示失败

优先检查：

1. Redirect URI 是否完全一致
2. Client ID / Secret 是否来自同一个 Microsoft 应用
3. Microsoft 云端点是否选对
4. 个人 Microsoft 账号是否误选了中国版端点
5. Microsoft 应用是否允许所需 delegated permissions

### 授权成功但无法解析 drive

检查 Drive 类型和目标字段：

- 默认个人 / 工作学校 OneDrive 通常不需要 Drive ID
- SharePoint site drive 需要 Site ID，除非已填写 Drive ID
- Microsoft 365 group drive 需要 Group ID，除非已填写 Drive ID
- Root item ID 留空或 `root` 最稳

### 需要经常重新授权

通常是 refresh token 不可用或被 Microsoft 拒绝。检查：

- 授权时是否包含 `offline_access`
- Microsoft 组织策略是否限制 refresh token
- 管理员是否在 Microsoft 侧撤销了授权
- Client Secret 是否轮换但 AsterDrive 策略没有更新
