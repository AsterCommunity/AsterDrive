# 草稿与历史记录

这个目录保存仍有参考价值、但不应被当成当前实现规范的内容。每份文档必须在开头明确标注状态，并指向当前权威文档或代码路径。

| 文档 | 状态 | 阅读目的 |
| --- | --- | --- |
| [静态配置密钥处理备忘](./static-config-secret-handling.md) | 草稿 | 评估敏感配置的脱敏、内存驻留和 `SecretString` 取舍 |
| [服务层模块化重构历史方案](./service-modularization-refactor-plan.md) | 历史快照 | 保存服务目录迁移前的分析和仍然有效的边界建议 |

草稿不代表已接受的路线，历史快照中的旧 `*_service` 名称也不是当前代码位置。当前服务层边界以[后端服务所有权边界](../architecture/backend-service-ownership.md)为准。
