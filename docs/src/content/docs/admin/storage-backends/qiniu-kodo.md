---
title: "七牛云 Kodo"
description: "通过 S3 兼容 API 配置七牛云 Kodo 存储策略。"
---

七牛云 Kodo connector 使用 **Kodo S3 兼容 API** 和 AWS SigV4。AsterDrive 仍负责文件、版本、配额、回收站和对象清理；Kodo 只保存对象内容。

本 connector 不使用 QBox、UpToken、原生表单上传或七牛原生 multipart REST。不要把七牛原生上传域名或 token 填入本表单。

## 配置前准备

在 Kodo 控制台为专用 bucket 创建具有最小读写和删除权限的 AccessKey / SecretKey。记录该 bucket 对应的 S3 兼容 endpoint 和该 endpoint 要求的 SigV4 region；endpoint 与 bucket 分开填写，不能把 bucket 重复拼入 endpoint。

开始时建议上传和下载均选 `relay_stream`。确认服务端读写正常后，再启用 `presigned`；浏览器直连时，Kodo endpoint 必须可从用户网络访问，并允许 AsterDrive 站点 origin 的 `GET`、`HEAD`、`PUT` 和 Range 所需请求/响应头。

## 创建策略

进入 `管理 -> 存储策略 -> 新建策略`，选择 **Qiniu Kodo**，填写：

| 字段 | 说明 |
| --- | --- |
| Kodo S3 endpoint | Kodo 提供的 HTTP(S) S3 兼容服务 endpoint，不含 bucket。 |
| Bucket | 目标 Kodo bucket 名称。 |
| 基础路径 | 可选对象前缀；留空使用 bucket 根。 |
| Kodo SigV4 签名区域 | Kodo endpoint 要求的 region，例如控制台或官方文档所示值。 |
| Path-style 寻址 | 默认开启，使用 `/bucket/key`；仅在 endpoint 已验证支持 virtual-hosted-style 时关闭。 |
| AccessKey / SecretKey | 专用于该策略的静态凭据；SecretKey 不会在读取策略时返回。 |

保存前运行草稿连接测试，保存后再运行已保存策略连接测试。测试会写入并删除一个临时对象，因此凭据必须同时有写入和删除目标前缀的权限。

## 验收与排障

在策略组中先绑定测试用户或团队，依次验证小文件上传、较大文件 multipart 上传、下载、Range 预览、删除和对象清理。启用 presigned 后，再从真实浏览器验证 PUT、GET/HEAD 和 Range 的 CORS 行为。

连接失败时，按 endpoint 可达性、bucket、region、path-style、AccessKey/SecretKey、权限和服务器时间的顺序检查。不要在 AsterDrive 日志、错误反馈或工单中粘贴 SecretKey 或完整签名 URL。

若改 endpoint、bucket、基础路径、region 或寻址方式，应新建目标策略并用存储迁移任务搬迁已有 blob；直接修改已有策略会使旧对象按原路径无法读取。
