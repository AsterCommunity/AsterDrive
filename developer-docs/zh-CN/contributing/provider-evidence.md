# Provider 外部证据维护

[证据索引](../reference/storage-provider-evidence.md)由 `docs/storage-provider-evidence.json` 生成。它拥有外部协议依据；connector descriptor 拥有实际能力声明，driver/connector 测试拥有可执行请求合同。三者不能互相冒充。当前索引覆盖全部内置 connector；local/remote 明确引用内部契约，SFTP 的 v3 文本是 IETF draft，不是正式 RFC。

## 稳定引用与所有权

引用格式为 `provider:<id>/<topic>`，例如 `provider:alibaba_oss/cors`。主题固定为 `objects`、`uploads`、`signing`、`cors`、`range_etag`、`errors`、`permissions`、`domains`。源码顶部和中英文教程链接到索引中的稳定 provider anchor；具体能力链接使用 `#<id>-<topic>`，例如 `#alibaba_oss-cors`。不要在通用前端维护另一份 provider 矩阵。

Storage maintainers 对索引、driver、connector 和测试共同负责；没有独立 CODEOWNERS 分配时由仓库维护者 review。新增 connector 必须同时进入真实 registry、生成的 connector manifest 和证据索引。未来 plugin 的 manifest/design review 必须列出相同的引用 ID、官方依据、版本、权限、请求/响应合同与云端验收状态；本次不虚构尚不存在的 plugin manifest runtime 字段。

## 证据记录

每个 provider 记录 API/签名版本、从 `Cargo.lock` 取出的 SDK/transport 精确版本、代码与本地测试、配置/凭据公开字段快照、关键协议/限制 guards，以及八类能力的中英结论。每条来源包含官方 URL、适用版本、访问尝试日期、内容复核日期、状态与说明。官方 SDK 的固定 commit 可以补充规范，但不能作为外部云能力唯一依据；第三方博客、搜索摘要和自动生成客户端不作为唯一权威来源。

- `verified`：维护者实际访问并核对了引用内容，填写 `reviewed_on`。仅表示文档内容已复核。
- `unverified`：未完成内容复核、无法访问、登录/地区/WAF 限制或依据不足。`reviewed_on` 必须为 null，说明原因；不得伪造成功日期。
- 链接报告的 `reachable`：只表示官方 HTTPS 入口本次返回 2xx。检查器还核对标题与关键限额文本，拒绝软 404、登录页和错误教程；其余内容变化仍需人工核对。报告不会自动修改内容复核日期。
- `cloud_acceptance.not_recorded`：本索引没有可引用的真实云验收记录，不能从本地 fixture、旧 PR 或 HTTP 200 推导成功。若为 `recorded`，必须提供仓库内的脱敏验收记录路径。

每次访问尝试和已验证内容距今最多 90 天；更新尝试日期不能刷新旧的内容复核日期。内容无法完成复核时保留 URL，将状态设为 unverified、清空 reviewed_on 并记录原因。一次链接探测失败只写入可达性报告；不能抹掉同日通过其他请求完成的内容复核。离线检查允许带说明的 unverified 条目并列出数量，避免网络故障伪装成协议已验证；线上链接检查遇到任何不可达来源返回非零，并上传详细报告。

## 修改与验证

```bash
# 编辑 curated JSON 后生成中英索引；不手动编辑生成页
node docs/scripts/storage-provider-evidence.mjs --write
# 快速离线检查：覆盖清单、字段/版本/关键常量、引用和新鲜度
make storage-provider-check
# 既有 descriptor/documentation 漂移 + 上述 provider 检查
make storage-docs-check
# 单独验证外部链接；失败明确标为 unverified
make storage-provider-links
# 检查器自身的拒绝边界与网络报告测试
node --test docs/scripts/storage-provider-evidence.test.mjs
```

`storage-provider-links.json` 是本地/CI 临时报表，不是内容审核事实源。每周 CI 检查外部链接并保留报表；PR 的离线门禁不依赖外部服务在线。修改 driver、connector、依赖或证据后，Docs Check 会执行离线检查。还需运行当前改动影响的 provider contract tests，不能用字符串 guard 替代真正的协议测试。

## Provider 升级顺序

1. 从官方 API/签名/权限/限额文档确认变化，记录版本、日期、差异和未验证项。冲突先调查，不靠 SDK 默认值推断。
2. 更新证据结论，再修改 connector/driver、descriptor 或 plugin contract。同步全部调用方，避免把 provider 语义塞进通用 UI。
3. 对普通请求、presigned、multipart/resumable、CORS 管理、Range/ETag、错误映射和权限分别添加或更新真实 fixture/独立签名向量。限制必须区分供应商限额与 Drive 的本地资源上限。
4. 有意识地更新依赖 pin、字段快照和关键 guards，生成索引，并审查中英文教程。不要为了检查通过盲目刷新快照。
5. 运行对应 provider tests、`make storage-docs-check`、链接检查和两个文档构建。源站 API、上传域、bucket-bound CNAME、CDN/custom download domain 分开验收，保留正例/拒绝例及缓存命中/未命中证据；缺凭据时明确未运行。

源站（origin）承载对象 API；上传域（upload endpoint）承载供应商上传协议；bucket-bound CNAME 可能是源站 API 的别名；CDN/custom download domain 是下载交付入口，具有独立回源授权、客户端签名、缓存和 CORS 边界。配置 CORS 成功不等于上传或 CDN 验收成功。
