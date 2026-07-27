---
description: "系统设置的网络访问分组：浏览器跨站访问规则（CORS），包括站点来源和浏览器扩展来源的放行方式。"
title: "网络访问"
---

所属入口：`管理 -> 系统设置 -> 网络访问`。其他分组和生效时机见 [系统设置](/reference/config/runtime/)。

这一组主要是浏览器跨站访问规则（CORS）。

只在这些场景才需要改：

- 浏览器页面和 AsterDrive 不在同一个域名下
- 想让别的站点在浏览器里直接调用 AsterDrive
- 浏览器扩展需要直接访问 WebDAV 或 API

`允许的跨域来源` 是由完整 origin 组成的数组，每个输入框填写一项，例如 `https://panel.example.com` 或 `chrome-extension://扩展ID`。当前支持 HTTP(S) 站点，以及 Chrome/Edge、Firefox 和 Safari Web Extension 的扩展来源。配置扩展时应填写完整扩展 ID，不要按协议放行所有扩展。

CORS 默认关闭，白名单也默认为空；此时服务端不会添加 CORS 响应头或拦截携带 `Origin` 的请求。启用 CORS 后，只有白名单中的精确来源会被允许。单独填写 `*` 可以允许任意来源，但不能同时开启跨域凭据。

:::tip[同站部署一般不用动]
大多数"前端页面和接口都在同一个站点里"的部署不需要碰这里。

接外部 WOPI 服务时，最常见的问题不是这里，而是 Office 服务回连不到 `公开站点地址` 生成的 WOPI URL。只有当浏览器控制台明确报 AsterDrive API 的 CORS 错误时，才需要把对应来源加到这里。
:::
