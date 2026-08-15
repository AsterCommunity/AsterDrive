---
title: "七牛云 Kodo"
description: "通过 S3 兼容 API 配置七牛云 Kodo 存储策略。"
---

七牛云 Kodo connector 使用 **Kodo S3 兼容 API** 和 AWS SigV4。AsterDrive 仍负责文件、版本、配额、回收站和对象清理；Kodo 只保存对象内容。

本 connector 不使用 QBox、UpToken、原生表单上传或七牛原生 multipart REST。不要把七牛原生上传域名或 token 填入本表单。

## 配置前准备

在 Kodo 控制台为专用空间创建 AccessKey / SecretKey，并按最小权限授予 `s3:GetObject`（同时覆盖 `GetObject` 和 `HeadObject`）、`s3:PutObject`、`s3:DeleteObject`、`s3:ListBucket`、`s3:AbortMultipartUpload` 和 `s3:ListMultipartUploadParts`。将权限范围限制在目标空间和允许的 `base_path`；运行时不要求 `s3:GetBucketLocation`。记录该空间对应的 **S3 空间名**、官方 S3 endpoint 和 Region ID。普通空间名称全局唯一时，S3 空间名与它相同；普通空间名称不全局唯一时，七牛会生成另一个全局唯一的 S3 空间名。请从 Kodo 控制台的空间概览或 [`Get Service`](https://developer.qiniu.com/kodo/manual/4087/compatible-s3-api#service-operation) 获取该值。

`s3:ListBucket` 是空间级操作，必须绑定空间 ARN，并通过 `s3:prefix` 限制可见前缀；对象读写和 multipart 操作必须绑定对象 ARN。以下 [`Bucket Policy`](https://developer.qiniu.com/kodo/6317/BucketPolicy) 示例假设 S3 空间名为 `example-space`，`base_path` 为 `tenant-a`：

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:ListBucket"],
      "Resource": ["arn:aws:s3:::example-space"],
      "Condition": {
        "StringLike": {
          "s3:prefix": ["tenant-a/*"]
        }
      }
    },
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts"
      ],
      "Resource": ["arn:aws:s3:::example-space/tenant-a/*"]
    }
  ]
}
```

把示例中的空间名和前缀替换为策略实际值。`base_path` 留空时，列表条件使用 `*`，对象资源使用 `arn:aws:s3:::example-space/*`。不要把 `s3:ListBucket` 放到对象 ARN，也不要把对象操作放到空间 ARN；Action 与 Resource 层级不匹配时，Kodo 会拒绝该策略。

Endpoint 只接受 HTTPS，并支持七牛官方的服务级 `https://s3.<region>.qiniucs.com` 和空间级 `https://<S3-空间名>.s3.<region>.qiniucs.com` 两种格式；`<region>` 必须与表单中的 SigV4 region 相同，空间级 host 中的名称必须与“七牛 S3 空间名”字段一致。两种输入都会规范化为 AWS SDK 使用的服务 endpoint，寻址方式由 connector 自动选择，避免重复拼接空间名。自定义 CNAME、非标准端口及带 path/query/fragment 的 URL 仍作为配置错误；其他 S3-compatible 服务应使用通用 S3 connector。

开始时建议上传和下载均选 `relay_stream`。确认服务端读写正常后，再启用 `presigned`；浏览器直连时，Kodo endpoint 必须可从用户网络访问，并允许 AsterDrive 站点 origin 的 `GET`、`HEAD`、`PUT` 和 Range 所需请求/响应头。

## 创建策略

进入 `管理 -> 存储策略 -> 新建策略`，选择 **Qiniu Kodo**，填写：

| 字段 | 说明 |
| --- | --- |
| Kodo S3 endpoint | 官方服务级 endpoint（如 `https://s3.cn-east-1.qiniucs.com`）或空间级 endpoint（如 `https://example-space.s3.cn-east-1.qiniucs.com`）；两种形式都会自动规范化。 |
| 七牛 S3 空间名 | 控制台空间概览或 `Get Service` 返回的全局唯一名称；它可能不同于普通 Kodo 空间名称。 |
| 基础路径 | 可选对象前缀；留空使用 S3 空间根。 |
| Kodo SigV4 签名区域 | Endpoint 主机名中的 Region ID，例如 `cn-east-1`；两者必须匹配。 |
| AccessKey / SecretKey | 专用于该策略的静态凭据；SecretKey 不会在读取策略时返回。 |

保存前运行草稿连接测试，保存后再运行已保存策略连接测试。测试会写入并删除一个临时对象，因此凭据必须同时有写入和删除目标前缀的权限。

## 验收与排障

在策略组中先绑定测试用户或团队，依次验证小文件上传、较大文件 multipart 上传、下载、Range 预览、删除和对象清理。启用 presigned 后，再从真实浏览器验证 PUT、GET/HEAD 和 Range 的 CORS 行为。

连接失败时，按 endpoint 可达性、S3 空间名、region 是否匹配、AccessKey/SecretKey、权限和服务器时间的顺序检查。寻址方式由 connector 自动处理。不要在 AsterDrive 日志、错误反馈或工单中粘贴 SecretKey 或完整签名 URL。

只有更换 S3 空间名、基础路径或实际目标存储位置时，才需要新建目标策略并用存储迁移任务搬迁已有 blob。对同一空间和基础路径，纠正与该空间匹配的 endpoint / region 不会改变对象 key，无需迁移；配置不匹配时直接修正配置。
