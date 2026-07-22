# 领域设计与契约

这组文档解释跨 route、service、repository、storage connector 和内部协议的领域边界。它们用于回答“为什么这样拆”和“多个实现必须共同遵守什么”，不是面向用户的配置操作手册。

## 身份与认证

- [外部认证模块](./external-auth.md)：provider descriptor、登录 flow、账号解析、邮箱补验和前后端边界。

## 存储与远端节点

- [远端存储目标与策略归属](./remote-storage-target-policy-ownership.md)：remote node、storage target 和 remote policy 的产品与工程边界。
- [存储 descriptor 与字段规范化契约](./storage-descriptor-normalization-contract.md)：后端权威字段、能力和 normalization 规则。
- [对象命名与 OneDrive 直接下载](./storage-object-naming-and-onedrive-direct-download.md)：object key、provider path 和直接下载边界。

## 上传完成

- [上传完成契约矩阵](./upload-finalization-contracts.md)：relay、multipart、presigned 和 provider resumable 路径的最终落账契约。

修改这些链路时，应同时检查对应 API、OpenAPI、前端生成类型和相关测试，不要在产品层重新硬编码 connector 能力。
