---
description: "AsterDrive 首次启动与第一个管理员：登录页按状态自动判断、第一个账号直接成为管理员、HTTP/HTTPS Cookie 引导策略，以及创建管理员后的下一步。"
title: "首次启动与第一个管理员"
---

:::tip[这一篇讲什么]
部署完成后第一次打开站点会发生什么、第一个管理员怎么来、HTTP/HTTPS 首次登录如何选择 Cookie 策略，以及创建管理员之后该做什么。
:::

## 登录页是按状态自动判断的

登录页不是固定的"登录"或"注册"页面，而是按当前状态走：

- **系统里还没有任何用户** —— 进入初始化流程，直接创建第一个管理员
- **系统里已有用户，输入的是现有账号** —— 登录
- **系统里已有用户，输入的是新账号，且管理员允许公开注册** —— 创建普通账号
- **管理员启用了外部认证提供商** —— 登录页会出现对应的外部登录入口
- **当前浏览器支持 Passkey** —— 登录页会显示 Passkey 登录入口
- **账号需要 MFA** —— 密码或外部身份通过后，还需要完成二次验证

需要注意：

- 第一个账号直接成为管理员，不走邮箱激活
- 后续公开注册的普通账号，要先点激活邮件才能登录（激活邮件依赖 [邮件投递](/admin/mail/)）
- 管理员关闭公开注册后，登录页只剩登录和找回密码

## 创建第一个管理员之后

第一个管理员创建成功后，系统会进入 `needs_storage` 状态：还没有默认存储策略，不能上传文件。下一步：

1. 在 `管理 -> 存储策略` 创建第一条存储策略并设为默认；概念和顺序见 [存储策略与策略组](/admin/storage-policies/)
2. 填 `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 为真实 HTTP(S) 来源
3. 如果要开放注册、找回密码或外部认证，先配通 [邮件投递](/admin/mail/)
4. 按 [首次启动检查](/ops/first-check/) 过一遍上线前状态

## 首次登录的 Cookie 策略

新安装默认允许直接通过 HTTP 创建管理员并登录，不需要额外添加环境变量：

```toml
[auth]
bootstrap_insecure_cookies = true
```

- **从 HTTP 来源创建管理员** —— 默认 `bootstrap_insecure_cookies = true` 时保持 `auth_cookie_secure = false`；显式设为 `false` 或数据库已有 `true` 时不会降级
- **从 HTTPS 来源创建管理员** —— 在随后的自动登录前自动把 `auth_cookie_secure` 提升为 `true`
- **要求进程从首次启动起就只发 Secure Cookie** —— 在数据库首次初始化前显式设 `bootstrap_insecure_cookies = false`

这个静态项**只影响数据库第一次写入** `auth_cookie_secure`。数据库里已经有运行时设置后，再改 `config.toml` 不会回写旧值；如果最初通过 HTTP 引导、后来切换到 HTTPS，请到 `管理 -> 系统设置 -> 认证与 Cookie` 开启“认证 Cookie 仅 HTTPS 发送”。

## 相关页面

- [部署方式选择](/start/choose-deployment/) —— 还没部署时先看这里
- [存储策略与策略组](/admin/storage-policies/) —— `needs_storage` 之后的主线
- [注册、登录与 SSO](/admin/auth-sso/) —— 注册开关、MFA 策略和外部认证
- [登录与会话](/reference/config/auth/) —— `config.toml` 里的静态认证密钥
