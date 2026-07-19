# 开发者文档

这里存放开发者向文档，例如 API 说明和架构说明。

`docs/` 目录只保留面向部署者和普通用户的使用手册，因此这些开发文档不会再参与用户文档站点构建。

## 入口

- [架构概览](./architecture.md)
- [关键模块设计说明](./module-designs.md)
- [后端服务所有权边界](./backend-service-ownership.md)
- [外部认证模块](./external-auth.md)
- [远端存储目标与策略归属](./remote-storage-target-policy-ownership.md)
- [存储 descriptor 与字段规范化契约](./storage-descriptor-normalization-contract.md)
- [对象命名与 OneDrive 直接下载](./storage-object-naming-and-onedrive-direct-download.md)
- [上传完成契约矩阵](./upload-finalization-contracts.md)
- [API 概览](./api/index.md)
- [标签 API](./api/tags.md)
- [测试与数据库后端](./testing.md)
- [Jemalloc 堆画像](./jemalloc-profiling.md)
- [静态配置密钥处理备忘](./static-config-secret-handling.md)
- [服务层模块化重构历史方案与当前目录映射](./service-modularization-refactor-plan.md)

## 当前状态说明

这些文档默认按当前代码实现维护。带有明确“草稿”“历史方案”状态的文档只用于记录决策背景或后续计划，不应直接当成当前已经落地的目录结构、接口或验收状态。
