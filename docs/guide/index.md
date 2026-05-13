# 使用指南

这一组文档按“你现在要做什么”来分，不按功能名硬背。

如果你是第一次来，先走 [快速开始](./getting-started)。如果服务已经跑起来，按自己的角色直接跳到对应入口。

## 第一次部署

你只想先把服务跑起来，看这几篇：

- [快速开始](./getting-started)：用最少步骤跑通登录、上传、分享和 WebDAV
- [部署概览](/deployment/)：正式上线前先选 Docker、systemd 还是直接运行二进制
- [首次启动检查](/deployment/runtime-behavior)：确认默认配置、存储策略、健康检查和后台任务是否正常
- [反向代理](/deployment/reverse-proxy)：准备挂 HTTPS、域名、WebDAV 或 WOPI 时看这一篇

## 日常使用

服务已经能打开后，普通用户优先看这里：

- [用户手册](./user-guide)：文件、工作空间、回收站、分享、WebDAV 和个人设置
- [常用流程](./core-workflows)：按真实场景串起常见操作
- [分享与公开访问](./sharing)：分享链接、密码、过期时间、下载次数
- [文件编辑](./editing)：浏览器内编辑、历史版本、WOPI 打开方式
- [上传与大文件](./upload-modes)：断点续传、对象存储直传和失败排查

## 管理员

管理员要先分清三类入口：后台页面、运行时系统设置、启动配置文件。

- [管理后台](./admin-console)：后台每个页面负责什么
- [配置总览](/config/)：`config.toml`、系统设置、存储策略和外部代理分别管什么
- [系统设置](/config/runtime)：站点、注册、Cookie、邮件、调度、回收站、WOPI、审计日志
- [存储策略](/config/storage)：本地、S3 / MinIO、远程节点和策略组
- [远程节点](./remote-nodes)：把另一台 AsterDrive 接成远程存储后端
- [自定义前端](./custom-frontend)：替换前端资源、注入自定义配置和处理 CSP

## 运维维护

上线后，稳定运行比“能打开页面”更重要。别嫌麻烦，出事再补就晚了。

- [运维 CLI](/deployment/ops-cli)：`doctor`、离线系统设置、跨数据库迁移、节点接入
- [升级与版本迁移](/deployment/upgrade)：升级前备份、升级后验证、失败回滚
- [备份与恢复](/deployment/backup)：数据库、配置、本地上传目录和恢复顺序
- [故障排查](/deployment/troubleshooting)：服务启动、上传、下载、分享、WebDAV、WOPI 和后台任务
- [性能基准与压测](/deployment/performance-benchmarking)：建立本地基线和复跑 smoke

## 项目本身

想知道 AsterDrive 为什么这么设计、适合谁、不适合谁，看 [关于 AsterDrive](./about)。

碰到错误码先看 [错误码处理](./errors)。如果错误发生在部署、反向代理、WebDAV 或 WOPI 场景里，再配合 [故障排查](/deployment/troubleshooting) 一起看。
