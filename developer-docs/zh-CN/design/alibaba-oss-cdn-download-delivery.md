# 阿里云 OSS CDN 下载交付

本文记录 Alibaba OSS 驱动的 CDN 下载域名支持与行为限制。

## 背景

阿里云 OSS 支持通过 CDN 加速域名分发对象，提供更好的下载性能和成本优势。
AsterDrive 需要在 `StorageDriver::generate_presigned_download` 中支持返回
CDN signed URL 而不是 OSS presigned URL。

## CDN 鉴权类型

阿里云 CDN 支持四种 URL 鉴权方式：

- **None**: 无鉴权（公开访问）
- **Type A**: `auth_key=timestamp-rand-uid-md5hash`
- **Type B**: `timestamp-md5hash`（路径后缀）
- **Type C**: `md5hash=md5(key+uri)&timestamp=hex(unix_time)`

每种类型的签名算法、字段位置和时间戳格式不同。详见：
- [鉴权方式 A](https://help.aliyun.com/document_detail/85113.html)
- [鉴权方式 B](https://help.aliyun.com/document_detail/85114.html)
- [鉴权方式 C](https://help.aliyun.com/document_detail/85115.html)

## 配置要求

`AlibabaOssDriverConfig` 新增字段：

- `cdn_domain`: HTTPS CDN 下载域名（例如 `https://cdn.example.com`）
- `cdn_auth_type`: 鉴权类型（`none` / `type_a` / `type_b` / `type_c`）

`AlibabaOssStaticCredentials` 新增字段：

- `cdn_auth_key`: CDN 鉴权密钥（Type A/B/C 需要）

### 安全约束

1. **CDN 域名必须使用 HTTPS**
   - OSS 内网回源不影响 CDN 边缘节点的用户访问协议
   - 明文 HTTP 会暴露下载路径和参数

2. **配置 CDN 域名时必须显式配置鉴权类型**
   - 驱动拒绝 `cdn_domain` 存在但 `cdn_auth_type` 为空的配置
   - 管理员必须明确选择 `none` 或具体鉴权类型
   - 防止误以为"私有 OSS bucket 回源"能保护 CDN 对象

3. **配置非 None 鉴权时必须提供密钥**
   - `validate_config` 检查 `cdn_auth_key` 非空

## CDN 使用策略

### 何时返回 CDN URL

`generate_presigned_download` 在以下条件**全部满足**时返回 CDN URL：

1. 驱动配置了 `cdn_domain` 和 `cdn_auth_type`
2. `DirectDownloadOptions.response_content_disposition` 为 `None`

### 为什么限制 response_content_disposition

阿里云 CDN 鉴权 URL 的签名**覆盖整个 URL**（包括 query 参数）：

- 无法在鉴权 URL 后追加 `?response-content-disposition=...`
- 追加参数会导致签名失败（403 Forbidden）

OSS presigned URL 支持响应覆盖参数，CDN 不支持。因此：

- **Inline 预览请求**：返回 CDN URL（无需覆盖）
- **Attachment 下载请求**：fallback 到 OSS presigned URL（保留语义）

这确保 `Content-Disposition` 语义始终正确，不会因为 CDN 优化而破坏下载文件名。

### Fallback 路径

当以下任一条件成立时，使用 OSS presigned URL：

1. 未配置 CDN
2. 请求包含 `response_content_disposition`（需要覆盖语义）
3. CDN 域名校验失败
4. CDN 鉴权类型解析失败

Fallback 是静默的（不报错），保证下载功能始终可用。

## 时间戳格式与有效期

### 时间戳格式

不同鉴权类型的时间戳格式不同：

- **Type A**: Unix 秒（十进制）
- **Type B**: UTC+8 时区的 `YYYYMMDDHHMM` 格式（例如 `202611021530`）
- **Type C**: Unix 秒的十六进制（例如 `673582ca`）

### 有效期计算

AsterDrive 的 `expires` 参数控制**鉴权 URL 的过期时间**：

```rust
let expiry_timestamp = Utc::now() + expires;
```

但**实际 CDN 缓存有效期**由两个因素决定：

1. **AsterDrive 的 `expires`**（鉴权 URL 过期时间）
2. **CDN 控制台配置的"鉴权 URL 有效时长"**（额外延长）

**实际有效期 = AsterDrive expires + CDN 控制台配置**

AsterDrive 无法读取 CDN 控制台配置，因此：

- 管理员应将 CDN 控制台的"鉴权 URL 有效时长"设置为较小值（例如 300 秒）
- 避免将 AsterDrive 的 `expires` 误认为是链接的完整 TTL

详见阿里云文档：
- [URL 鉴权配置](https://help.aliyun.com/document_detail/85117.html)

## 测试覆盖

### 单元测试

- 各鉴权类型的签名生成（包括时间戳格式）
- CDN 域名校验（HTTPS 要求、有效 URL）
- 鉴权类型解析（有效值、错误处理）
- 配置验证（域名/鉴权类型/密钥组合）

### 集成测试

- Inline 预览返回 CDN URL
- Attachment 下载返回 OSS presigned URL
- 未配置 CDN 时返回 OSS presigned URL
- 无效配置时 fallback 到 OSS presigned URL

### 未测试项

- **CDN 边缘节点的实际鉴权行为**（需要真实 CDN 环境）
- **CDN 控制台配置的有效时长叠加**（需要控制台操作）

## 与 OSS presigned URL 的对比

| 特性 | OSS presigned URL | CDN signed URL |
|------|-------------------|----------------|
| 下载性能 | 直连 OSS | CDN 边缘加速 |
| 成本 | OSS 流量费 | CDN 流量费（通常更低） |
| 响应头覆盖 | 支持 `response-*` 参数 | 不支持（签名限制） |
| 签名算法 | OSS V4 | CDN 鉴权 |
| HTTPS 要求 | 可选 | 必须 |

## 兼容性

### 旧策略兼容

- `AlibabaOssDriverConfig` 的 CDN 字段标记 `#[serde(default)]`
- 旧策略反序列化时 CDN 字段为 `None`，保持原有行为

### 跨数据库

- 不涉及 migration（仅驱动配置）
- SQLite/PostgreSQL/MySQL 无差异

## 实现位置

- **Connector**: `src/storage/connectors/alibaba_oss.rs`
  - 配置解析
  - trim 和默认值处理

- **Driver**: `src/storage/drivers/alibaba_oss/mod.rs`
  - 配置验证
  - CDN/OSS 路径选择

- **CDN Signing**: `src/storage/drivers/alibaba_oss/cdn_signing.rs`
  - 鉴权类型解析
  - 签名生成
  - 时间戳格式化

## 参考资料

- [阿里云 CDN URL 鉴权](https://help.aliyun.com/document_detail/85117.html)
- [鉴权方式 A](https://help.aliyun.com/document_detail/85113.html)
- [鉴权方式 B](https://help.aliyun.com/document_detail/85114.html)
- [鉴权方式 C](https://help.aliyun.com/document_detail/85115.html)
