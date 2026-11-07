# 预览内容与集成应用的信任边界

本文区分普通用户上传的文件与管理员配置的展示应用。它们都可能显示脚本内容，但授权来源和允许的能力不同。不能通过给所有 iframe 添加严格 sandbox 或禁用外部地址来代替文件内容隔离。

## 所有权与边界

| 输入 | 所有者 | 执行与访问边界 |
| --- | --- | --- |
| 用户上传的 HTML、XHTML、SVG、PDF 等原始字节及 MIME | 文件内容提供者 | 获准读取文件不表示允许它以 Drive origin 执行脚本 |
| 自定义 URL template、允许的 origin、展示模式 | 管理员 | 保留配置的同源/跨源应用及 iframe/new-tab 行为；文件名等变量继续编码，不能变成模板结构 |
| WOPI action/discovery、额外表单字段 | 管理员与启用的文档应用 | 保留启动 POST、文件范围 token、命名 iframe、查看器自身 origin、脚本、表单、存储、弹窗、下载和现有剪贴板权限 |
| 浏览器 Blob/data 显示 URL | 前端资源层与具体 viewer | 原始 HTTP 响应的 CSP 不能视为重新创建的 Blob 文档的隔离证明 |

`allow-same-origin` 保留 iframe 文档自己的 origin；跨源文档不会因此变成 Drive 同源。管理员部署同源应用则属于主动信任，不能与普通上传内容混为一谈。基础自定义 iframe 与可信文档查看器的现有 sandbox 配置分别保持，不在文件漏洞修复中擅自改变。

## WebDAV 原件与历史版本

GET/HEAD、单段/多段 Range 和条件响应在实际返回边界设置 `X-Content-Type-Options: nosniff`。HTML 与全部 XML MIME（`application/xml`、`text/xml`、`+xml`，含 XHTML/SVG）使用文件产品已有的 `Content-Security-Policy: sandbox`，包括不可变历史版本及 304。原始字节、MIME、文件名、ETag、Range 和下载行为保持；不会把 Office HTML 页面、WOPI JSON 或管理员应用响应一起套上该策略。

## SVG 图片显示

SVG 放入 `<img>` 的图片上下文与作为独立文档导航具有不同能力。原始 SVG 的主站 Blob URL 可从图片地址导航为文档，因此前端资源层为 SVG 生成保留原始字节的 data URL。图片仍保留矢量、动画和尺寸；独立打开时属于 opaque origin，无法获得 Drive origin 的 DOM、存储与会话读取权限。这不是 SVG 清洗，也不承诺删除文件中的脚本；原件下载和服务端受隔离的直接展示仍使用原始内容。

网络结果与持久化 thumbnail cache 恢复经过同一显示 URL 规则，304 不应重新引入不安全表示。普通 PNG/JPEG 等保留 Blob URL。data URL 不保存为文件身份，也不包含 token 或授权决定。

## PDF 工具栏

文件名和服务端保存的 MIME 都可能来自上传者，不能决定新标签页的文档类型。一个 PDF 可以被 PDF.js 正常解析，同时尾部含有被 HTML 解释器接受的内容。PDF viewer 为已加载 Blob 创建独立的 `application/pdf` 表示 URL，工具栏只打开该 URL，不直接导航上传者类型的原件 URL。原字节与下载文件名不变，正常 PDF、octet-stream PDF、翻页、缩放、打印及下载继续工作；替换/卸载时释放 viewer 创建的 URL，旧文件不能串到新文件。

## WOPI 新窗口

外部打开直接以 `target="_blank" rel="noopener"` 提交真实启动表单。先 `window.open` 一个隔离窗口，再向另一个命名目标 POST，不能保证实际查看器没有 opener，还可能留下空白窗口。修复只作用于外部导航：嵌入模式仍提交到原命名 iframe；POST token、TTL、附加字段与现有 referrer 行为保持，不增加 `noreferrer`，不收紧查看器 sandbox。

## 验证边界

- 浏览器反例只使用隔离实例与本地状态标记，验证不可信内容不能取得主站 origin；不接触真实用户数据或外部服务。
- WebDAV 同时证明字节和协议头不变，覆盖当前文件与历史版本的 GET/HEAD/Range/304。
- SVG 证明图片显示与独立打开隔离，PDF 证明混合内容不按 HTML 执行，以及普通预览、下载、分享不回归。
- 管理员应用通过实际配置 API 安装测试查看器，验证同源和允许的跨源 embed 脚本/表单；WOPI 验证 iframe 脚本/存储、POST 字段、权限和外部窗口无 opener。
- 本地查看器与协议 fixture 不替代真实 Collabora、OnlyOffice、Microsoft/Google 及具体自定义应用的部署验收。

## 外部查看器的分享委托

`/pv` 的分享预览 token 仅包含内部 share ID、固定 file ID、到期时间与随机 nonce，不包含原分享 bearer。签名在服务端绑定当前分享 token、密码状态与文件/根范围；消费时从 writer 重查分享过期、团队生命周期及文件仍在当前范围内。外部查看器无需用户登录，但只能读取被委托的那个文件；普通分享权限不能从预览 token 中恢复。旧 bearer-bearing 预览链接不再接受，可重新生成短期预览 URL。原分享本身不自动变更或撤销。

预览仍不消耗下载次数。次数限制只控制对应下载操作，不是已交付内容的防复制保护。
