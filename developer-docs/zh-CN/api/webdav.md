# WebDAV API 与协议能力

WebDAV 相关内容可以分成三块：账号、挂载入口、协议能力。

当前协议层已经拆在 `src/webdav/**` 下：`mod.rs` 负责 Actix 挂载和方法分派，认证在 `auth.rs`，文件系统适配在 `fs.rs` / `file.rs` / `dir_entry.rs`，路径解析在 `path_resolver.rs`，锁系统在 `db_lock_system.rs`，DeltaV 子集在 `deltav.rs`。

## 账号接口

以下路径都相对于 `/api/v1`，且都需要认证。

### 个人空间账号

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/webdav-accounts` | 列出当前用户的 WebDAV 账号 |
| `POST` | `/webdav-accounts` | 创建 WebDAV 账号 |
| `DELETE` | `/webdav-accounts/{id}` | 删除 WebDAV 账号 |
| `POST` | `/webdav-accounts/{id}/toggle` | 启用或停用账号 |
| `GET` | `/webdav-accounts/settings` | 读取当前挂载前缀和客户端可直接使用的挂载地址 |
| `POST` | `/webdav-accounts/test` | 测试一组 WebDAV 凭据 |

### 团队空间账号

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/webdav-accounts` | 列出团队 WebDAV 账号 |
| `POST` | `/teams/{team_id}/webdav-accounts` | 创建团队 WebDAV 账号 |
| `DELETE` | `/teams/{team_id}/webdav-accounts/{account_id}` | 删除团队 WebDAV 账号 |
| `POST` | `/teams/{team_id}/webdav-accounts/{account_id}/toggle` | 启用或停用团队 WebDAV 账号 |

常用点：

- 创建账号时，`password` 为空会自动生成随机密码
- 明文密码只在创建时返回一次
- 个人账号的 `root_folder_id` 为空表示可访问整个用户空间；团队账号的 `root_folder_id` 为空表示可访问整个团队空间
- 创建账号时如果传了 `root_folder_id`，服务端会校验该文件夹确实属于账号所在的个人或团队工作空间
- `/toggle` 没有请求体，每调用一次就在启用 / 停用之间切换
- `/settings` 会返回两个字段：
  - `prefix`：服务端当前实际启用的挂载前缀
  - `endpoint`：面向客户端的可访问地址；如果配置了 `public_site_url`，这里会是绝对 URL，否则返回相对路径。多来源配置下，服务端会用当前请求 Origin（scheme + host[:port]）精确匹配 `public_site_url` 列表。命中时返回对应来源下的 WebDAV 地址，未命中时回退第一项。
- `/test` 用来先验账号密码，不必真的挂载客户端
- `GET /webdav-accounts` 是分页接口，支持 `limit` 和 `offset`
- `GET /teams/{team_id}/webdav-accounts` 也是分页接口，支持 `limit` 和 `offset`
- 团队成员可以创建团队 WebDAV 账号；普通成员只能列出、删除、切换自己创建的账号，团队 `owner` / `admin` 可以列出和管理该团队的全部 WebDAV 账号
- 团队 WebDAV 账号必须通过 `/teams/{team_id}/webdav-accounts/*` 管理；个人 `/webdav-accounts/{id}` 接口遇到团队账号会返回无权操作

创建请求示例：

```json
{
  "username": "dav-demo",
  "password": null,
  "root_folder_id": 12
}
```

## 挂载地址

默认 WebDAV 路径是：

```text
/webdav
```

完整地址例如：

```text
http://localhost:3000/webdav
```

如果修改了 `[webdav].prefix`，挂载地址也会一起变化。

## 协议能力

当前已覆盖常见 WebDAV 方法：

- `PROPFIND`
- `PROPPATCH`
- `MKCOL`
- `PUT`
- `GET`
- `HEAD`
- `DELETE`
- `COPY`
- `MOVE`
- `LOCK`
- `UNLOCK`
- `OPTIONS`

另外还补了最小 DeltaV 子集：

- `REPORT` 的 `DAV:version-tree`
- `VERSION-CONTROL`
- `OPTIONS` 的 `DAV: version-control`

这部分直接复用 `file_versions`，所以客户端可以读取历史版本树。

限制也很直接：

- `REPORT version-tree` 只支持文件
- 当前不是完整 DeltaV 服务器，只是最小可用子集
- `/webdav/` 挂载根只是一个虚拟入口，不是持久化的文件夹实体。`PROPFIND /webdav/` 可以列目录和读取 live DAV 属性，但 `PROPPATCH /webdav/` 明确返回 `403 Forbidden`；自定义 dead properties 只支持具体文件或文件夹。

## 认证与运行时开关

- Basic Auth：使用 WebDAV 专用账号，可限制到 `root_folder_id`
- Bearer JWT：复用普通登录态，不受 `root_folder_id` 限制
- `webdav_enabled = false` 时，WebDAV 请求会直接返回 `503`
- `webdav_block_system_files_enabled = true` 时，WebDAV 写入 / 移动 / 复制会按 `webdav_block_system_file_patterns` 拦截系统文件名，默认包含 `.DS_Store`、`._*`、`Thumbs.db`、`desktop.ini`、`$RECYCLE.BIN` 等常见客户端垃圾文件；REST 文件夹列表不会应用这层过滤

如果部署在反向代理后面，还要确认代理层允许 WebDAV 方法和相关请求头，见 [反向代理部署](../../../docs/deployment/reverse-proxy.md)。
