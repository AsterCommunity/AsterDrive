---
layout: home
description: AsterDrive 官方文档首页，按快速开始、使用指南、管理员配置和部署运维组织，覆盖 Docker、systemd、WebDAV、WOPI、远程节点和备份恢复。

hero:
  name: AsterDrive
  text: 官方文档中心
  tagline: Rust + React 构建，从单机部署开始，按需要接入团队空间、S3、Azure Blob、OneDrive、SFTP、WebDAV、WOPI 和远程从节点。
  actions:
    - theme: brand
      text: 快速开始
      link: /guide/getting-started
    - theme: alt
      text: 使用指南
      link: /guide/
    - theme: alt
      text: 部署概览
      link: /deployment/

features:
  - title: 第一次部署
    details: 从快速开始到正式上线，先跑通服务，再处理 HTTPS、数据目录、健康检查和首次验收。
    link: /guide/getting-started
  - title: 日常使用
    details: 文件、工作空间、上传、分享、回收站、WebDAV 和在线编辑，按真实使用路径组织。
    link: /guide/
  - title: 功能地图
    details: 按后端能力梳理身份访问、文件工作空间、上传存储、预览处理和系统运维，适合管理、排障和二开定位。
    link: /features/
  - title: 管理员配置
    details: 分清 config.toml、后台系统设置、存储策略、策略组、存储后端、邮件、远程节点各自负责什么。
    link: /config/
  - title: 运维维护
    details: Docker、systemd、反向代理、升级、备份、故障排查和运维 CLI 放在同一条维护路径里。
    link: /deployment/
---

## 先认识它

AsterDrive 是一个基于 Rust 和 React 的轻量自托管云盘。你可以从默认 SQLite + 本地存储的单机部署开始，之后按需要接入 PostgreSQL / MySQL、S3 兼容对象存储、Azure Blob Storage、腾讯云 COS、Microsoft Graph 支持的 OneDrive / SharePoint drive、SFTP 文件服务器、团队空间、WebDAV、WOPI 在线预览编辑，以及远程从节点存储。

它不是完整协作套件或多主集群系统，当前重点是把个人和小团队的文件管理、分享、上传、预览、存储策略和日常运维路径做清楚。

## 按目的走

### 我只是想先跑起来

从 [快速开始](/guide/getting-started) 走一遍。它会带你完成启动服务、创建第一个管理员、上传文件、试分享、检查 WebDAV 和跑一轮基础验收。

如果你已经决定正式部署，直接看 [部署概览](/deployment/)。那一组文档会把 Docker、systemd、反向代理、上线检查、升级和备份放在同一条线上讲清楚。

### 我已经登录了，想知道怎么用

从 [使用指南](/guide/) 进入。普通用户优先看 [用户手册](/guide/user-guide) 和 [常用流程](/guide/core-workflows)，单独问题再跳到团队权限、分享、编辑、在线预览、上传或 WebDAV。

### 我要管一个实例

先看 [管理后台](/guide/admin-console)，再看 [配置总览](/config/)。AsterDrive 的配置分成启动配置、后台运行时设置、存储策略、策略组、存储策略后端和外部网络环境，按层看会清楚很多。

如果你正在接新的存储策略后端，看 [存储策略后端](/storage/)。

### 我要按功能模块找文档

看 [功能地图](/features/)。它按后端能力把身份访问、文件工作空间、上传存储、预览处理和系统运维串起来，适合管理员排障、定位后端模块或准备二开。

### 我要上线或排障

上线前按 [部署概览](/deployment/) 选方式，再补 [反向代理](/deployment/reverse-proxy)、[首次启动检查](/deployment/runtime-behavior)、[生产上线检查](/deployment/production-checklist)、[备份与恢复](/deployment/backup)。已经出问题就直接去 [故障排查](/deployment/troubleshooting)，看到错误码再配合 [错误码处理](/guide/errors)。

### 我看不懂某个词，或者不知道该查哪里

先看 [术语表](/guide/glossary) 和 [常见问题速查](/guide/faq)。这两页不是让你从头读的，是让你少走弯路的。

---

::: tip 一句话
**别给自己的数据增加心智负担**——这是我们做 AsterDrive 的初衷。
:::
