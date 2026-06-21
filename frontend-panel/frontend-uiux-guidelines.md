# AsterDrive 前端 UI/UX 规范

本文定义 AsterDrive 前端的产品体验、视觉层级、交互规则和工程验收标准。它服务于 0.3.0 及之后的前端改动。

AsterDrive 是自托管云存储系统，不是营销型 SaaS 官网。界面应当像一个可靠的文件管理工作台：安静、清楚、可重复操作、适合长时间使用。

## 1. 产品气质

AsterDrive 的界面应该符合这些关键词：

- 操作型
- 清楚
- 稳定
- 高密度但不拥挤
- 桌面端效率优先
- 移动端触控友好
- 管理后台偏工具化
- 视觉克制但精致

避免这些方向：

- 营销页式 hero 和大卡片堆叠
- 大面积紫蓝渐变、玻璃拟态、装饰光斑
- 以装饰为目的的嵌套 card
- 只为了“高级感”降低信息密度
- 用动画掩盖层级不清

## 2. 信息层级规则

所有按钮和入口必须能归入下面五类之一。

### 2.1 全局操作

作用范围：整个应用或当前工作空间。

位置：

- TopBar
- 用户菜单
- 全局搜索

例子：

- 全局搜索
- 用户设置
- 管理后台
- 上传/任务中心
- 工作空间切换

### 2.2 导航入口

作用范围：切换当前位置或页面。

位置：

- Sidebar
- Tabs
- Breadcrumb

例子：

- 文件夹树
- 快速分类
- 回收站
- 我的分享
- 任务
- 设置

### 2.3 当前页面操作

作用范围：当前页面或当前文件夹。

位置：

- PageHeader
- PageToolbar

例子：

- 当前目录上传
- 新建文件夹
- 刷新
- 排序
- 视图切换
- 页面筛选

### 2.4 对象操作

作用范围：当前文件、文件夹、用户、策略、任务等具体对象。

位置：

- 行内更多按钮
- 右键菜单
- SelectionToolbar
- DetailPanel

例子：

- 下载
- 移动到
- 复制到
- 重命名
- 管理标签
- 分享
- 删除
- 编辑

### 2.5 危险操作

作用范围：会删除、覆盖、撤销授权、破坏状态或难以恢复的操作。

规则：

- 使用 destructive 视觉状态。
- 需要明确确认。
- 复杂 Dialog 内使用 inline confirmation，不再嵌套 AlertDialog。
- 文案必须说明将影响什么对象。

## 3. 桌面端交互

桌面端优先效率，但不能只服务高级用户。

### 3.1 文件列表和网格

- 单击主区域打开或选中，具体行为要在 list/grid 中一致。
- 右键打开对象菜单。
- 行内或卡片上必须有更多按钮，作为右键的可见替代。
- hover 状态要清楚，但不能导致布局位移。
- 选中状态要比 hover 更强。
- 拖拽只在明确可接收的位置显示反馈。

### 3.2 SelectionToolbar

多选后显示选择工具条。

应展示：

- 已选数量
- 清除选择
- 主要对象操作
- 更多菜单

不应展示：

- 当前目录新建/上传
- 全局工具
- 与选中对象无关的入口

### 3.3 右键菜单

右键菜单是效率入口，不是唯一入口。

规则：

- 和行内更多菜单使用同一组动作定义。
- 分组顺序稳定。
- 危险操作放在底部并用 destructive。
- 多选右键菜单应反映多选语义。

推荐顺序：

1. 打开/预览
2. 下载
3. 分享
4. 移动到/复制到
5. 管理标签
6. 重命名/版本历史/详情
7. 删除

## 4. 移动端交互

移动端必须作为触控界面设计，不是桌面端压缩版。

### 4.1 基本规则

- 不依赖右键。
- 不依赖长按作为主要操作。
- 可点击目标不小于 44px。
- 行内更多按钮必须可见或容易发现。
- 选择态使用底部工具条。
- Dialog 尽量全屏或近全屏。
- footer 固定，列表单独滚动。

### 4.2 文件行

移动端文件行建议结构：

```text
[icon/thumbnail] [name + meta] [more]
```

规则：

- 点击主区域打开或预览。
- 更多按钮打开对象操作。
- 多选模式下显示 checkbox 或选择态。
- 不把大量操作直接铺在行内。

### 4.3 多选

进入多选后：

- 底部显示 selection bar。
- 左侧显示已选数量和清除按钮。
- 右侧显示一个主要动作和更多按钮。
- 删除、移动、复制、标签管理等放入更多菜单或 action sheet。

不要让用户通过长按猜测如何进入多选。

### 4.4 Dialog

手机 Dialog 规则：

- `max-width` 不应造成横向挤压。
- header 固定在顶部。
- 列表区域独立滚动。
- footer 固定在底部。
- 主按钮和取消按钮在窄屏下使用整宽或两列布局。
- 关闭按钮位置必须符合项目统一 Dialog 模式。

## 5. Dialog 规范

### 5.1 Dialog 类型

#### ConfirmDialog

用于简单确认。

特点：

- 内容短。
- 无复杂列表。
- 只有取消和确认。
- 不承载搜索、编辑、分页。

#### FormDialog

用于创建或编辑单个对象。

特点：

- 表单字段清楚分组。
- footer 固定。
- 保存时按钮 loading。
- 表单错误靠近字段显示。

#### PickerDialog

用于选择目标，例如移动到、复制到、选择策略。

特点：

- 可搜索。
- 可导航。
- 当前选择明确。
- 确认按钮在 footer。

#### ManagerDialog

用于管理一组资源，例如标签库、成员、权限。

特点：

- 搜索/过滤区固定在列表上方。
- 列表单独滚动。
- 支持分页或加载更多。
- 编辑和删除使用行内状态。
- 不嵌套确认弹窗。

### 5.2 Footer 规则

- Dialog footer 不随内容滚动。
- 主按钮放右侧，移动端可整宽。
- 有未保存变更时，保存按钮才可用。
- 取消按钮必须能明确放弃草稿。
- 异步提交时禁用相关按钮。

### 5.2.1 异步提交规则

创建、保存、确认、登录、重置密码等异步提交动作必须有同步防重复提交门闩。

规则：

- 不只依赖按钮 `disabled` 或 React `useState` 的 `submitting` 判断防重复提交；state 更新存在异步空窗，连续点击或回车提交仍可能重复进入。
- 表单提交、按钮点击、键盘回车必须共享同一个 pending guard。
- 单例提交优先使用 `usePendingAction` 这类 ref-backed hook；按资源 ID 操作优先使用已有 `usePendingId`。
- pending guard 必须在进入异步动作前同步置位，并在 `finally` 中释放。
- UI 仍然要展示 pending 状态，例如禁用相关按钮、显示 loading icon 或替换按钮文案。
- 如果某个流程有批量 key、operation type、跨列表刷新等额外语义，可以保留局部 ref-backed 状态，但不要退回只靠 `useState`。

### 5.3 滚动规则

复杂 Dialog 的滚动只能发生在内容列表区域。

不允许：

- header 被滚走。
- footer 被滚走。
- 搜索框被列表滚走。
- body 和内部列表同时滚动造成滚动冲突。

### 5.4 删除确认

复杂 Dialog 内删除资源时，优先使用 inline confirmation。

示例结构：

```text
Tag Name                    Edit Delete
Are you sure?               Cancel Confirm delete
```

不推荐双击删除。双击删除可发现性差，也容易误触。

## 6. 搜索规范

搜索是发现层，不只是输入框。

### 6.1 搜索应支持的维度

- 关键词
- 文件/文件夹类型
- 文件分类
- 标签
- 标签匹配模式
- 所属位置或工作空间

### 6.2 搜索结果

结果项应展示：

- 名称
- 类型图标或缩略图
- 所在路径
- 关键 meta
- 标签摘要
- 可用动作入口

### 6.3 空状态

搜索空状态应区分：

- 尚未输入搜索条件
- 没有匹配结果
- 标签库为空
- 加载失败

不要用同一句“还没有可用标签”覆盖所有情况。

## 7. 标签交互规范

标签不是独立文件浏览器。

标签相关需求拆成两类：

- 对象操作：给文件或文件夹管理标签。
- 搜索发现：用标签找到文件或文件夹。

规则：

- 文件/文件夹的标签管理入口属于对象操作。
- 标签库管理属于管理型 Dialog。
- 按标签找文件应该走搜索。
- 不做阉割版 `/tags` 文件浏览页，除非未来重新定义为完整的标签管理中心。
- 标签默认颜色可以按名称 hash 生成。
- 标签展示应使用颜色点或色块 + 名称，避免只显示数量。

## 8. 视觉规范

### 8.1 色彩

主方向：

- light-first
- 中性背景
- 清楚边框
- 克制阴影
- selection 状态明确
- danger 状态明确

避免：

- 大面积单一 hue
- 大面积深蓝/紫蓝渐变
- 过多半透明叠层
- 仅靠颜色表达状态

建议沉淀语义 token：

```css
--surface-page
--surface-panel
--surface-toolbar
--surface-elevated
--surface-selected
--border-subtle
--border-strong
--text-primary
--text-secondary
--text-tertiary
--z-dropdown
--z-popover
--z-dialog
--z-toast
```

### 8.2 间距和密度

文件管理器和后台页面需要较高信息密度。

规则：

- 页面级区域保持清楚分隔。
- 工具条紧凑但按钮点击区域足够。
- 表格行高度稳定。
- 卡片圆角不超过现有设计系统约束。
- 不在 card 里嵌套 card。

### 8.3 字体

规则：

- 不随 viewport 宽度缩放字体。
- 小面板内不用 hero 级标题。
- 按钮文字不能溢出。
- 长文件名必须 truncate 或换行策略明确。
- 字母间距默认 0，不使用负 letter spacing。

### 8.4 图标

规则：

- 优先使用项目统一 `Icon` 封装。
- icon-only 按钮必须有 `aria-label`。
- 不手写 SVG，除非没有现有图标且确有必要。
- 不用 emoji 作为功能图标。

### 8.5 动效和状态过渡

动效用于说明状态变化和层级关系，不用于装饰，也不用于掩盖信息架构问题。

当前项目已有模式：

- `components/ui/dialog.tsx`：Dialog overlay 使用 `duration-100` + `fade-in/fade-out`，内容使用 `fade + zoom` 的 `data-open` / `data-closed` 入退场。
- `components/ui/dropdown-menu.tsx`、`components/ui/context-menu.tsx`、`components/ui/select.tsx`、`components/ui/tooltip.tsx`：浮层使用 `duration-100` 到 `duration-150`，以 `fade + zoom + side slide` 表达锚点关系。
- `pages/file-browser/FileBrowserToolbar.tsx`：SelectionToolbar 使用 `120ms` fade in/out，并在退出期保留旧内容，避免工具条瞬间闪回面包屑。
- `components/common/AnimatedCollapsible.tsx`、`components/folders/folder-tree/AnimatedTreeGroup.tsx`：展开/折叠使用 `max-height`、`grid-template-rows`、`opacity`、`transform` 组合，展开通常 `180ms-220ms`，收起通常 `160ms` 左右。
- `components/files/FileInfoDialog.tsx`：侧边详情面板使用 `width + opacity + transform`，约 `280ms`，用于表达工作区侧栏进入。
- `index.css`：主题切换只过渡颜色、边框、阴影等视觉 token，约 `160ms`；浏览器 View Transition 只做 root fade，约 `180ms`。

规则：

- Dialog、AlertDialog、Dropdown、ContextMenu、Select、Tooltip 等浮层必须保留入场和退场动效。扁平化 Dialog 结构时，不要删除 overlay/content 的 `data-open`、`data-closed`、`animate-in`、`animate-out`、`fade`、`zoom` 等状态类。
- 常规 hover、focus、selected、disabled 状态只过渡颜色、边框、阴影、透明度；默认 `120ms-180ms`。
- 小浮层入退场使用 `100ms-150ms`，Dialog/Sheet/侧栏使用 `100ms-220ms`，复杂高度或工作区面板切换可到 `240ms-300ms`，不要超过用户能感知为等待的长度。
- 进入可以使用 `cubic-bezier(0.22, 1, 0.36, 1)` 或 `ease-out`，退出可以使用 `ease-in`。同一类组件要保持一致，不要每个 Dialog 自己发明 easing。
- 高度、宽度、折叠类动效优先复用 `AnimatedCollapsible` 或现有 `grid-template-rows` 模式；不要用内容自然高度直接过渡导致跳变。
- 文件列表、表格行、工具条、卡片 hover 不允许因为动效改变布局尺寸；可以改变背景、边框、阴影、透明度，谨慎使用 `transform`，且不能影响相邻元素。
- SelectionToolbar、保存条、上传条、侧边详情、移动端导航抽屉这类状态切换区域必须有明确入退场，避免瞬间替换造成层级闪烁。
- loading 状态使用项目统一 spinner 或 skeleton；spinner 只表示正在处理，不用于普通装饰。
- 长时间循环动画只允许用于明确的 loading、播放中、任务进行中状态。普通业务页面不要使用循环装饰动画。
- 所有非必要动效必须支持 `prefers-reduced-motion`：Tailwind 类使用 `motion-reduce:animate-none` 或 `motion-reduce:transition-none`，JS 动画读取 `window.matchMedia("(prefers-reduced-motion: reduce)")` 后将 duration 置为 `0`。
- 不用动画作为唯一信息来源。动画消失后，状态仍必须通过文本、图标、颜色、selected/focus 样式或布局位置表达清楚。

## 9. 状态规范

每个重要组件都要考虑这些状态：

- loading
- loading more
- empty
- filtered empty
- error
- saving
- disabled
- selected
- hover
- focus
- destructive confirming

异步按钮规则：

- 提交中显示 loading。
- 提交中禁用重复点击。
- 成功反馈用 toast 或局部状态。
- 错误反馈不要只写到 console。

## 10. 可访问性规范

最低要求：

- 普通文本对比度不低于 WCAG AA。
- 所有交互元素有可见 focus。
- icon-only 按钮有 `aria-label`。
- 键盘 tab 顺序符合视觉顺序。
- Dialog 打开后焦点进入 Dialog。
- Escape 关闭规则一致。
- 表单字段有 label。
- 不用 hover 作为唯一信息来源。

键盘快捷键：

- 全局搜索可以支持 `Ctrl/Command + K` 和 `/`。
- 文件列表后续可以逐步补方向键导航。
- 快捷键不能和输入法组合态冲突。

## 11. 文案和 i18n

规则：

- 所有用户可见文本走 i18n。
- 按钮文案表达动作结果，例如“移动到”“复制到”“管理标签”。
- 不同上下文不要复用含义不清的 key。
- 空状态文案必须说明当前状态，不要泛化。
- 危险操作确认文案必须包含影响对象或数量。

例子：

- 单/多选标签操作统一使用“管理标签”。
- 移动和复制使用“移动到”“复制到”，避免只写“移动”“复制”。
- 搜索标签无结果应区分“没有匹配标签”和“标签库为空”。

## 12. 前端工程规范

代码约束：

- 使用 React 19 + Vite + TypeScript 当前栈。
- 禁止 TS enum，使用 `as const` 对象或 union type。
- 类型导入使用 `import type`。
- OpenAPI 生成类型从 `frontend-panel/src/types/api.ts` 使用。
- 不手写生成接口的替代类型。
- 优先复用 `components/ui`、`components/common`、`lib` 下已有模块。
- 日志使用项目 logger，避免散落 `console.warn/error`。

测试和验证：

- 前端检查使用 `cd frontend-panel && bun run check`。
- 组件行为变更补对应测试。
- 关键 UI 变更至少检查 375px、768px、1024px、1440px。
- 涉及 3D 或 canvas 时必须做非空渲染检查；普通业务 UI 不引入无关 3D。

## 13. 新功能设计检查清单

提交或评审新前端功能前，先回答：

- 这个入口属于全局、页面、对象还是危险操作？
- 桌面端不用右键能不能完成？
- 移动端不用长按能不能完成？
- Dialog 是否有固定 header/footer？
- 内容多时滚动区域是否唯一且明确？
- 操作提交前是否有草稿和保存语义？
- loading、empty、error 状态是否存在？
- 状态切换、Dialog/浮层入退场、SelectionToolbar/保存条等是否保留项目既有动效，并支持 `prefers-reduced-motion`？
- icon-only 按钮是否有 `aria-label`？
- 文案是否走 i18n？
- 这个组件是否复用了已有模式，而不是新造一次性布局？

答不上来就先不要写 UI。先把交互层级想清楚，代码自然会少很多。
