---
layout: home

hero:
  name: AsterDrive
  text: 文档中心
  tagline: 自托管云存储的部署、使用、管理和运维手册。先选路径，再看细节。
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
  - title: 管理员配置
    details: 分清 config.toml、后台系统设置、存储策略、策略组、邮件、远程节点各自负责什么。
    link: /config/
  - title: 运维维护
    details: Docker、systemd、反向代理、升级、备份、故障排查和运维 CLI 放在同一条维护路径里。
    link: /deployment/
---

## 按目的走

### 我只是想先跑起来

从 [快速开始](/guide/getting-started) 走一遍。它会带你完成启动服务、创建第一个管理员、上传文件、试分享、检查 WebDAV 和跑一轮基础验收。

如果你已经决定正式部署，直接看 [部署概览](/deployment/)。那一组文档会把 Docker、systemd、反向代理、升级和备份放在同一条线上讲清楚。

### 我已经登录了，想知道怎么用

从 [使用指南](/guide/) 进入。普通用户优先看 [用户手册](/guide/user-guide) 和 [常用流程](/guide/core-workflows)，单独问题再跳到分享、编辑、上传或 WebDAV。

### 我要管一个实例

先看 [管理后台](/guide/admin-console)，再看 [配置总览](/config/)。AsterDrive 的配置分成启动配置、后台运行时设置、存储策略、策略组和外部网络环境，混在一起看只会把自己绕晕。

### 我要上线或排障

上线前按 [部署概览](/deployment/) 选方式，再补 [反向代理](/deployment/reverse-proxy)、[首次启动检查](/deployment/runtime-behavior)、[备份与恢复](/deployment/backup)。已经出问题就直接去 [故障排查](/deployment/troubleshooting)，看到错误码再配合 [错误码处理](/guide/errors)。

---

::: tip 一句话
**别给自己的数据增加心智负担**——这是我们做 AsterDrive 的初衷。
:::
