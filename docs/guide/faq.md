# 常见问题速查

这页不是完整排障手册，只负责把问题分流到正确文档。已经出问题时，先按现象找入口，会比从头翻完整本文档更快。

## 服务和登录

| 现象 | 先看哪里 | 常见原因 |
| --- | --- | --- |
| 服务起不来 | [故障排查：服务起不来](/deployment/troubleshooting#服务起不来) | 配置文件路径、数据库连接、端口占用、目录权限 |
| 健康检查失败 | [首次启动检查](/deployment/runtime-behavior) | 数据库未 ready、迁移未完成、默认策略没初始化 |
| 登录后反复掉线 | [登录与会话](/config/auth) / [系统设置](/config/runtime#认证与-cookie) | Cookie HTTPS 设置、公开站点地址、反向代理 Host 处理 |
| 新用户注册后不能登录 | [系统设置](/config/runtime#用户管理) / [邮件](/config/mail) | 开启了邮件激活但邮件没发通 |

## 上传、下载和存储

| 现象 | 先看哪里 | 常见原因 |
| --- | --- | --- |
| 小文件能传，大文件失败 | [上传与大文件](./upload-modes) | 反向代理大小限制、超时、分片大小、临时目录空间 |
| 对象存储直传失败 | [存储策略](/config/storage) / [上传与大文件](./upload-modes) | S3 CORS、`ETag` 暴露、浏览器来源没放行 |
| 远程节点策略上传失败 | [远程节点](./remote-nodes) | 传输方式不通、直连地址错误、默认接收落点未应用 |
| 容量显示不对 | [运维 CLI：doctor](/deployment/ops-cli#部署检查doctor) | 存储用量计数漂移，需要深度检查 |

## 分享、WebDAV 和在线编辑

| 现象 | 先看哪里 | 常见原因 |
| --- | --- | --- |
| 分享链接域名不对 | [系统设置：公开站点地址](/config/runtime#站点配置) | 没填公开站点地址，或第一项不是主要公开入口 |
| WebDAV 连不上 | [WebDAV](/config/webdav) / [反向代理](/deployment/reverse-proxy#webdav-代理时不要漏掉什么) | 代理没放行 WebDAV 方法、路径前缀或上传上限不对 |
| Office 文件打不开 | [文件编辑](./editing) / [系统设置：预览应用](/config/runtime#站点配置) | WOPI 服务不能回连 AsterDrive，公开站点地址或 CORS 配错 |
| 升级后页面显示异常 | [前端资源缓存](/deployment/frontend-assets) | 浏览器、CDN 或代理缓存了旧资源 |

## 配置和维护

| 现象 | 先看哪里 | 常见原因 |
| --- | --- | --- |
| 不知道该去后台还是改文件 | [配置总览](/config/) | 启动配置、系统设置、存储策略和反向代理混在一起了 |
| 后台进不去但要改配置 | [运维 CLI：config](/deployment/ops-cli#离线系统设置config) | 需要离线查看、校验或写入系统设置 |
| 准备升级但担心无法回滚 | [升级与版本迁移](/deployment/upgrade) / [备份与恢复](/deployment/backup) | 未准备旧二进制/镜像、配置、数据库和上传目录备份 |
| 术语看不懂 | [术语表](./glossary) | 先分清主控、从节点、存储策略、策略组、接收落点这些词 |

## 还没解决怎么办

先收集这些信息，再去开 issue 或找人看：

- AsterDrive 版本
- 部署方式：Docker、systemd、直接运行二进制
- 数据库后端：SQLite、PostgreSQL、MySQL
- 存储策略类型和后端配置
- 反向代理类型和关键配置
- 浏览器控制台错误、服务端日志和对应错误码

有错误码就先看 [错误码处理](./errors)。没有错误码但现象明确，就从 [故障排查](/deployment/troubleshooting) 开始。
