# AsterDrive 开发者文档

这里是面向 AsterDrive 贡献者、集成开发者和维护者的源码级文档库，覆盖仓库架构、领域设计、API、协议契约、测试基础设施和诊断入口。普通用户、部署者和管理员应优先阅读[用户文档](https://drive.astercosm.com/)。

## 从哪里开始

| 你的目标 | 建议入口 |
| --- | --- |
| 第一次接触仓库，想建立整体心智模型 | [架构概览](./architecture/index.md) → [关键模块设计](./architecture/module-designs.md) |
| 判断一段后端逻辑应该放在哪一层 | [后端服务所有权边界](./architecture/backend-service-ownership.md) |
| 查找 REST、WebDAV、WOPI 或内部协议 | [API 概览](./api/index.md) |
| 修改存储、上传或远端节点链路 | [领域设计与契约](./design/README.md) |
| 运行数据库、WebDAV 或诊断测试 | [测试与诊断](./testing/index.md) |
| 查阅尚未落地的讨论或历史决策背景 | [草稿与历史记录](./records/README.md) |

## 文档库

### 架构与边界

- [架构概览](./architecture/index.md)：节点模式、分层、启动链路、配置和数据流。
- [关键模块设计](./architecture/module-designs.md)：文件、上传、任务、存储和协议模块的内部形状。
- [后端服务所有权边界](./architecture/backend-service-ownership.md)：route、service、domain、repository、storage 和 protocol 的职责边界。

### 领域设计与契约

- [领域设计与契约索引](./design/README.md)
- [外部认证模块](./design/external-auth.md)
- [远端存储目标与策略归属](./design/remote-storage-target-policy-ownership.md)
- [存储 descriptor 与字段规范化契约](./design/storage-descriptor-normalization-contract.md)
- [对象命名与 OneDrive 直接下载](./design/storage-object-naming-and-onedrive-direct-download.md)
- [上传完成契约矩阵](./design/upload-finalization-contracts.md)

### API 与协议

[API 概览](./api/index.md)按身份认证、文件工作流、团队与分享、后台任务、管理接口、WebDAV、WOPI、健康检查和 follower 内部协议组织全部接口页。机器可读规范仍以 OpenAPI 导出为准。

### 测试与诊断

- [测试与数据库后端](./testing/index.md)
- [WebDAV 合规与兼容性检查](./testing/webdav-compliance-testing.md)
- [Jemalloc 堆画像](./testing/jemalloc-profiling.md)

### 草稿与历史记录

- [草稿与历史记录索引](./records/README.md)
- [静态配置密钥处理备忘](./records/static-config-secret-handling.md) — **草稿**
- [服务层模块化重构历史方案](./records/service-modularization-refactor-plan.md) — **历史快照**

## 文档状态

| 状态 | 含义 |
| --- | --- |
| 当前实现 | 默认状态；应与当前代码、路由和测试保持一致 |
| 草稿 | 记录待讨论方向，不代表已经接受或落地 |
| 历史快照 | 保留决策背景；正文中的旧名称和路径不作为当前实现依据 |
| 待翻译 | 中文内容可用，英文站暂时通过 Starlight fallback 展示中文原文 |

修改实现时，先以当前代码和测试确认事实，再同步对应文档。不要因为旧文档写过某个结构，就把已经迁移掉的路径重新引回来。
