# AsterDrive Frontend Panel

React 前端管理面板，嵌入 Rust 二进制分发。

## 技术栈

- React 19 + TypeScript (tsgo native-preview)
- Vite 8 + Tailwind CSS 4
- shadcn/ui (Base UI) + Radix 风格组件
- zustand 5 (状态管理)
- axios (HTTP 客户端)
- uppy 5 (文件上传)
- biome (lint + format)

## 开发命令

```bash
bun install              # 安装依赖
bun run dev              # 开发服务器 (proxy 到 :3000)
bun run build            # 构建 (tsgo + vite → dist/)
bun run check            # biome lint
bun run check:fix        # biome 自动修复
bun run generate-api     # 从 OpenAPI spec 生成 TypeScript SDK
```

## 关键约定

### TypeScript 规则
- `erasableSyntaxOnly: true` — **禁止 TS enum**，用 `as const` 对象
- `verbatimModuleSyntax: true` — 类型导入必须用 `import type`
- tsgo (非 tsc) 做类型检查，biome (非 eslint) 做 lint
- biome 用 tab 缩进，double quote

### API SDK 自动生成
- 后端跑 `cargo test --features openapi --test generate_openapi` → 生成 `generated/openapi.json`
- 前端跑 `bun run generate-api` → 生成 `src/services/api.generated.ts`
- 所有后端 schema 类型从 `@/types/api.ts` re-export
- **禁止手写重复的接口类型**，必须从 SDK schema 导入
- `api.generated.ts` 在 biome 忽略列表中，不要手动修改

### 类型导入示例
```typescript
import type { UserInfo, ShareInfo, StoragePolicy } from "@/types/api";
```

### API 调用
- 统一通过 `src/services/http.ts` 的 `api` 对象
- `api.get/post/patch/put/delete` 自动 unwrap `ApiResponse<T>` 并处理错误码
- 401 自动 refresh token（拦截器处理）
- 所有请求带 `withCredentials: true`（HttpOnly cookie 认证）

### 存储策略能力
- 存储策略 UI 的连接测试、字段显示、上传工作流、授权入口、远端节点绑定、S3 传输策略、存储原生处理、driver action 可用性等能力判断，必须优先来自后端 storage connector descriptor 的 `capabilities`、`fields`、`upload_workflows`、`actions`、`connection_tests` 等元数据。
- 不要在前端新增 `driver_type === "s3" || driver_type === "azure_blob" || ...` 这类白名单/矩阵来推断能力。已有临时兜底也只能用于 descriptor 缺失时的保守兼容，不能扩大成新的事实来源。
- descriptor 缺失时，如果必须根据表单内容做兼容判断，只能使用字段是否存在、用户是否实际填写了相关字段、URL scheme 这类局部信号；不要把 S3/Azure/COS/Remote/OneDrive 的能力关系重新硬编码回前端。

### 组件库
- shadcn/ui (Base UI 底层)，用 `render` prop 而非 `asChild`
- 用 `@/components/ui/` 下的组件
- 安装新组件用 `bunx shadcn@latest add <component>`

### 路由结构
```text
/login          — LoginPage (LoginGuard)
/               — FileBrowserPage (ProtectedRoute)
/admin/*        — Admin 页面 (AdminRoute, admin only)
/s/:token       — ShareViewPage (公开，无需认证)
```

### 状态管理
- authStore — 登录/登出/token 状态
- fileStore — 文件/文件夹列表 + 导航

### 公共模块

新代码应优先使用以下公共模块，不要在组件里重新定义：

**工具函数** (`src/lib/`)
- `format.ts` — `formatBytes()` / `formatDate()` / `formatDateAbsolute()` / `formatDateShort()` / `formatDateTime()`，不要内联 `new Date(...).toLocaleDateString()` 等
- `formatBatchToast.ts` — `formatBatchToast(t, operation, result)`，批量操作结果 toast 通知（支持 move/copy/delete/restore/purge），不要内联 toast 逻辑
- `constants.ts` — 共用常量（`SIDEBAR_SECTION_PADDING_CLASS`、`PAGE_SECTION_PADDING_CLASS`、`ADMIN_ICON_BUTTON_CLASS`、`DRAG_MIME` 等）
- `uploadPersistence.ts` — 断点续传 localStorage 持久化（`saveSession` / `removeSession` / `loadSessions`）
- `preferenceSync.ts` — 偏好设置 debounce 同步（`queuePreferenceSync` / `cancelPreferenceSync`）
- `logger.ts` — 统一日志（`logger.warn` / `logger.error` 始终输出，`logger.debug` 仅 DEV），优先用此模块而非直接 `console.warn/error`

**公共组件** (`src/components/common/`)
- `StatusBadge` — 通用状态徽章（active/expired/disabled），自动翻译 + 颜色映射
- `ConfirmDialog` — 确认弹窗基础组件
- `SkeletonTable` / `SkeletonCard` / `SkeletonFileGrid` / `SkeletonFileTable` — 加载骨架屏
- `EmptyState` — 空状态展示
- `AdminSurface` — admin 页面内容容器
- `RoleBadge` / `UserStatusBadge` — 用户角色/状态徽章（`getRoleBadgeClass` / `getStatusBadgeClass`）
- `ItemCheckbox` — 统一 checkbox 组件（替代手写 SVG）
- `Icon` — 统一图标封装（react-icons/pi），不要手写 `<svg>`

**Hooks** (`src/hooks/`)
- `useConfirmDialog<T>` — 确认弹窗状态管理（`confirmId` / `requestConfirm` / `dialogProps`），消除重复的 `useState + ConfirmDialog` 模式
- `useApiList<T>` — 列表加载状态管理（`items` / `loading` / `reload` / `setItems`），消除重复的 `useState + useCallback + useEffect` 加载模式
- `useApiError` — API 错误处理（`handleApiError`）
- `useBlobUrl` — 缩略图 blob URL 管理（202 自动重试）
- `useSelectionShortcuts` — 批量选择快捷键

### 前端目录
```text
src/
├── components/
│   ├── admin/      UserDetailDialog
│   ├── common/     ConfirmDialog, EmptyState, SkeletonTable, StatusBadge,
│   │               AdminTableList, RoleBadge, UserStatusBadge, Icon, ItemCheckbox
│   ├── layout/     AppLayout, PageHeader, AdminLayout
│   ├── files/      FileList, UploadArea, ShareDialog
│   ├── folders/    FolderTree
│   └── ui/         shadcn 组件
├── config/         app.ts (apiBaseUrl + STORAGE_KEYS)
├── hooks/          useApiError, useConfirmDialog, useApiList, useBlobUrl,
│                   useSelectionShortcuts
├── lib/            utils.ts (cn), format.ts (formatBytes/formatDate*),
│                   formatBatchToast.ts, constants.ts, uploadPersistence.ts,
│                   preferenceSync.ts, logger.ts
├── pages/
│   ├── admin/      AdminUsersPage, AdminPoliciesPage, AdminSettingsPage,
│   │               AdminLocksPage, AdminSharesPage
│   ├── LoginPage.tsx
│   ├── FileBrowserPage.tsx
│   ├── MySharesPage.tsx
│   ├── TrashPage.tsx
│   ├── WebdavAccountsPage.tsx
│   └── ShareViewPage.tsx
├── router/         路由定义
├── services/
│   ├── http.ts             axios 封装 + 拦截器
│   ├── api.generated.ts    [自动生成] OpenAPI SDK 类型
│   ├── authService.ts      登录/注册/me
│   ├── fileService.ts      文件/文件夹 CRUD
│   ├── adminService.ts     管理后台 API
│   ├── shareService.ts     分享链接 API
│   ├── trashService.ts     回收站 API
│   ├── batchService.ts     批量操作
│   └── webdavAccountService.ts  WebDAV 账号管理
├── stores/         zustand stores (authStore, fileStore, themeStore)
└── types/
    └── api.ts      从 api.generated re-export + ErrorCode 常量
```

## 错误码域
- 0: 成功
- 1000: 通用
- 2000: 认证
- 3000: 文件
- 4000: 存储策略
- 5000: 文件夹
- 6000: 分享

## i18n 国际化

### Namespace 分层
```text
src/i18n/locales/{zh,en}/
├── core/            # [初始加载] 通用 UI：按钮、表头、状态、主题/语言、确认弹窗
├── files/           # [初始加载] 文件浏览器：上传、批量操作、回收站、预览、排序、文件信息
├── auth/            # [初始加载] 登录/注册/初始化
├── validation.json  # [初始加载] 表单验证消息（zod schema 引用，体量小，单文件保留）
├── errors/          # [初始加载] API 错误提示（ErrorCode → 翻译映射）
├── offline.json     # [初始加载] 离线/PWA 更新提示（体量小，单文件保留）
├── admin/           # [延迟加载] 管理后台
├── webdav.json      # [延迟加载] WebDAV 账号管理页面（体量小，单文件保留）
├── settings/        # [延迟加载] 设置页面描述文案
├── share/           # [初始加载] 分享对话框 + 我的分享页面 + 分享查看页面
├── search.json      # [延迟加载] 搜索页面（体量小，单文件保留）
└── tasks/           # [初始加载] 后台任务中心
```

体量较大的 namespace 按子类拆成目录内多个 JSON，但运行时仍合并成同一个 flat namespace。也就是说调用侧继续使用 `t("files:upload_success")`、`t("admin:overview_total_users")`，不要把 key 改成深层路径。

### 命名约定
- **core** 是默认 NS（`defaultNS: "core"`），`t("key")` 无需前缀
- 其他 NS 用显式前缀：`t("admin:key")`、`t("webdav:key")`、`t("share:key")` 等
- key 名保留原前缀以自文档化：`webdav_endpoint`、`settings_theme_light_desc`、`my_shares_title`
- **禁止**在多个 NS 中重复定义同一个 key（core 与 files 之间的少数共享 key 除外）
- 大 namespace 新增翻译时放到对应子类文件，并同步 `src/i18n/index.ts` 的 `SPLIT_NAMESPACE_PARTS` 顺序；未拆的小 namespace 才继续使用单个 `{namespace}.json`

### 加载策略
- 初始加载：core、files、auth、validation、errors、offline、share、tasks（首屏和常驻入口必需）
- 延迟加载：admin、webdav、settings、search（按需加载）
- 切换语言时，先确保初始 NS 加载完成，再异步加载延迟 NS

### 新增翻译的判断规则
- 全局通用（按钮、状态词、表头）→ `core.json`
- 仅文件浏览器用 → `files.json`
- 仅管理后台用 → `admin.json`
- 新页面专属 → 新建独立 NS（加入 `DEFERRED_NAMESPACES`）

## UI / UX 约定
- 后台页面优先做得**宽一点、松一点、大气一点**，不要为了塞信息把内容挤成小卡片或密集按钮墙
- 优先用“**左侧概览 + 右侧主内容**”或“**清晰分区的大面板**”组织详情页，而不是把所有字段平均切成几个小块
- 列表页要**整洁、克制**：字段分层明确，常看信息直接展示，低频操作收进详情面板或二级交互
- **不要把所有数据堆在一张表里**；当字段开始变多时，优先拆到 detail dialog / detail panel
- 可点击区域尽量做大，不要逼用户只点小图标、小箭头或极窄按钮
- 交互元素数量要克制：能用展示态 + 详情编辑解决的，不要在列表里塞一排输入框、下拉框、开关
- 视觉上优先保证留白、层级、对齐和主次关系，避免“功能都有，但界面很挤”
- admin 弹窗默认可适当做宽，优先保证信息布局舒展；不要为了保守尺寸把表单和详情区压得很局促
- 横向安全边距尽量统一，不要每个页面/侧栏/表格各写一套 magic spacing
- 共用横向边距优先收口到 `src/lib/constants.ts`，当前已约定：`SIDEBAR_SECTION_PADDING_CLASS`、`PAGE_SECTION_PADDING_CLASS`
- `AdminSurface` 默认自带 `flex-1`，更适合列表/主内容区；像 about / info 这类按内容高度展示的页面，如果不希望卡片撑满剩余屏幕，记得在页面级显式覆盖成 `flex-none`
- `PAGE_SECTION_PADDING_CLASS` 当前只有横向 padding，没有纵向 padding；信息展示页若内容贴着 `AdminSurface` 上边界，不要改全局常量，优先在该页局部补 `py-*`
- 管理后台这类表格优先使用“**分割线满宽，只有文本内容留边距**”的模式：不要给整张表外层包一层 `px-*` 让横线一起缩进去
- 这类表格默认通过 `src/components/ui/table.tsx` 的首列/末列 padding 规则统一；若表格放在 `AdminSurface` 里，优先用 `padded={false}` 关闭容器默认横向 padding，不要手写 `p-0 md:p-0`
- checkbox 列、占位列等例外再用 `first:pl-*` / `last:pr-*` 做局部覆盖
- 侧栏内的树、导航、设置列表等内容块要与统一边距对齐，避免根节点/表格内容紧贴左右边界
- 侧栏树节点、设置表格、管理表格等若出现左右贴边或根节点/首行未撑满整行，优先检查并复用上述常量，不要单页手搓额外 padding

### Flex 滚动链完整性
- **ScrollArea / `overflow-auto` 要求从视口根到滚动容器的每一层 flex 容器链不断裂**
- 每个中间层必须同时有 `flex flex-col`（成为 flex column 容器）和 `min-h-0 flex-1`（在父 flex 中受约束并占满剩余空间）
- 如果某一层只写了 `min-h-0 flex-1` 但漏了 `flex flex-col`，它就不是 flex 容器，子元素的 `flex-1` 失效，高度不受约束，滚动容器会被内容撑大
- 典型正确链: `h-screen flex flex-col → flex min-h-0 flex-1 flex-col → ScrollArea min-h-0 flex-1`
- **新增包裹容器时（section / AdminSurface / ContextMenuTrigger 等）务必检查是否需要 `flex flex-col`**

### Base UI DropdownMenuItem + SVG 颜色问题
- `DropdownMenuItem`（Base UI `Menu.Item`）用 `data-highlighted=""` 属性表示 hover，不是原生 CSS `:hover`
- 基础样式包含 `[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4`，在 SVG 上创建了独立的 CSS 规则层
- 这导致 `data-[highlighted]:text-destructive` 设置的父级 `color` 无法通过 `currentColor` 正确传播到 SVG 的 `fill`——SVG 不变色，只在 `data-highlighted` 移除时短暂闪红（`transition-colors` 回弹）
- `**:` 通配选择器（`data-[highlighted]:**:text-destructive`）也无效，因为基础样式里 `not-data-[variant=destructive]:focus:**:text-accent-foreground` 的 specificity 更高
- **结论：需要 hover 变色的 SVG 图标场景，不要用 `DropdownMenuItem`，改用原生 `<button>` + CSS `:hover`，`currentColor` 继承天然生效**

### Base UI Select.Value 回显 raw value 的坑
- `Select.Value` 在 **没有** 给 `Select.Root` 传 `items` 映射时，默认回显当前 `value` 本身，不一定会显示 `SelectItem` 里的 label
- 如果 `value` 是策略 ID、策略组 ID、文件夹 ID、分页大小、枚举值等，trigger 里就会直接显示 `1` / `2` / `__all__` / `relay_stream` 这类 raw value
- 这个坑会同时影响 dialog 和普通页面，不只是某一个表单；策略组选择、用户状态/角色筛选、WebDAV 根目录、分页大小、分享过期时间等都踩过
- **正确写法**：给 `Select` 传 `items={[{ label, value }]}`，再渲染同一份 options 到 `SelectItem`；不要假设 `SelectItem` 的 children 会自动成为 trigger label
- 当 options 来自接口数据时，优先先生成 `const options = data.map(...)`，然后同时给 `items={options}` 和 `options.map(...)` 使用，避免 label/value 两套来源漂移
- 若当前选项可能不在当前已加载页里（例如滚动分页加载的策略下拉），除了 `items` 外，还要先把“当前已选项”并回 options，否则 trigger 可能回不到正确名称
