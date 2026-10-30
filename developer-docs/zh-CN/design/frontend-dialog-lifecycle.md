# 前端弹窗生命周期

业务弹窗使用 `components/ui/dialog-lifecycle.tsx` 的 `DialogLifecycle` 管理目标与内容子树。`open=false` 是停止业务工作的信号，退出完成才释放表单、列表、标题及 object URL。初始关闭时不挂载内容；状态依次为 `closed → entering → open → exiting → disposed`。

## 使用方式

外层只接收打开状态和目标，内容组件拥有草稿与展示资源：

```tsx
function ResourceDialog(props: ResourceDialogProps) {
  return (
    <DialogLifecycle
      open={props.open}
      target={props}
      identity={props.resource?.id}
      onOpenChange={props.onOpenChange}
    >
      {({ target, active, onOpenChange }) => (
        <ResourceContent {...target} open={active} onOpenChange={onOpenChange} />
      )}
    </DialogLifecycle>
  );
}
```

`target` 是当前展示输入；关闭时保留最后一份有效输入，父层可以立即清空自己的选择。`identity` 标识请求与 mutation 对应的业务目标，不要传每次 render 都新建的对象。已打开时更换目标不重建 frame；内容的数据加载应按资源身份更新，迟到响应不得覆盖新目标。

Base UI `Dialog` / `AlertDialog` 自动将 `onOpenChangeComplete` 接入最近的 owner。`ManagerDialogShell` 继续负责布局，复用相同入口。业务组件不再转发退出完成回调，不再维护 retained hook、退出后的 reset 或复制 CSS 时长的 timer。嵌套 primitive 隔离完成通知，子弹窗不能销毁父内容。

非 Base UI 的桌面 inspector 和内联未保存提示使用 `useDialogMotion`；它等待实际 `getAnimations().finished`，包括取消、无动画和 reduced-motion。不得使用固定延时推算卸载时刻。路由卸载直接释放子树，并使旧回调失效。

## 业务工作与展示资源

- 请求与订阅依赖 `active`，关闭立即 abort 或使结果失效；不清空已显示内容。显示资源在内容组件卸载的 cleanup 中释放。
- 异步事件在开始时捕获 `useDialogActivity()` 返回的检查函数。await 后更新本地状态、显示失败或关闭前检查它；不要在 await 后读取一个已经换成新会话的 ref。
- 已提交 mutation 的缓存失效、列表刷新和成功通知仍应执行。把这些副作用放在检查之前；旧会话不能向新目标注入表单错误或调用关闭回调。
- 由页面/controller 持有的受控表单与异步工作使用同一 `useDialogActivity(identity, open)` 检查，presentation owner 保留其最后展示快照。共享业务缓存的读取可继续完成，但关闭后应解除当前 UI 订阅和丢弃迟到结果。
- 预览资源 hook 在退出中保留已加载句柄、文本与 blob URL；关闭时停止当前 UI 请求、轮询和订阅，退出后释放展示引用。长期缓存仍遵守原有缓存策略。
- 生成密码和 WebDAV 密码在退出时立即遮蔽为圆点；复制与业务操作随关闭失效。外框和非敏感说明仍保留到退出完成。
- Base UI 继续负责 ARIA、Escape、遮罩、焦点回归及滚动锁。不要用普通 div 替换模态 primitive。

## 迁移清单

下列入口均已接入统一 owner；同名组件的个人、团队、管理员和公开分享调用方使用相同契约。

| 入口 | 迁移状态与所有权 |
| --- | --- |
| GlobalSearchDialog | 已迁移；query、分类、标签随内容子树保留 |
| AvatarCropDialog | 已迁移；裁剪状态与 source object URL 在内容组件中释放 |
| TagManagerDialog、TagLibraryManagerDialog | 已迁移；目标、草稿、列表与嵌套库管理统一释放 |
| ConfirmDialog / AlertDialog | 已迁移；删除手写标题/描述冻结 ref |
| CreateFileDialog、CreateFolderDialog、RenameDialog | 已迁移；退出不清空输入，重开创建新内容实例 |
| ShareDialog、EditShareDialog | 已迁移；删除业务退出 reset 和 retained hook |
| ArchiveTaskNameDialog、OfflineDownloadDialog、BatchTargetFolderDialog | 已迁移；删除 open/closed key 和父层目标保留 |
| FolderPolicyDialog、FolderIconDialog | 已迁移；退出不重置表单与目标 |
| FileInfoDialog | 已迁移；桌面 inspector 等待实际动画，移动端 Dialog 使用 Base UI 完成信号 |
| VersionHistoryDialog | 已迁移；行数据随子树保留，切换资源的请求隔离 |
| FilePreview / FilePreviewDialog | 已迁移；个人与公开分享父层不再手工保留目标 |
| UnsavedChangesGuard | 已迁移；实际动画完成替代退出 timer |
| DownloadCenter 下载方式选择 | 已迁移；页面内嵌选择目标保留到退出完成 |
| WebdavCreateAccountDialog、WebdavCredentialsDialog | 已迁移；个人与团队入口一致，退出遮蔽密码 |
| CreateUserDialog、InviteUserDialog、GeneratedPasswordDialog、CreateTeamDialog | 已迁移；受控表单快照保留，旧请求不能关闭新弹窗 |
| AdminTaskDetailDialog、AdminTaskCleanupDialog | 已迁移；任务目标由统一 owner 保留 |
| AdminFilesPage 文件详情、Blob 详情 | 已迁移；删除页面 retained hook |
| StoragePolicyDialog setup | 已迁移；page 模式仍是页面，未伪装成弹窗 |
| StoragePolicyMigrationDialog、StoragePolicyRecoveryDialog | 已迁移；受控证据与表单保留，controller 隔离迟到结果 |
| PolicyGroupMigrationDialog、PolicyGroupSimulationDialog | 已迁移；全部页面调用方接入 |
| PolicyGroupEditorForm 分类帮助 | 已迁移；页面内嵌 Dialog |
| RemoteNodeRemoteStorageTargetDialog | 已迁移；远端节点内嵌目标表单 |
| PreviewAppsAddDialog、PreviewAppsEditDialog、PreviewAppsWopiDiscoveryDialog、PreviewAppsUrlTemplateVariablesDialog | 已迁移；编辑器内嵌及嵌套帮助入口 |
| MailTemplateVariablesDialog、TestEmailDialog | 已迁移；设置页内嵌入口 |
| ManagerDialogShell | 布局层，透过同一 Dialog primitive 接入，不建立第二个 owner |

当前仓库没有独立 `Sheet` primitive；移动端详情由 Dialog 承载。所有懒加载弹窗必须让外层组件常驻，用 `open` 表达关闭，不在调用处以 `{target && <Dialog />}` 提前移除 owner。

## 验证

`dialog-lifecycle.test.tsx` 验证会话切换、旧退出通知、StrictMode、路由卸载、动画取消/无动画和 mutation 成功副作用。GlobalSearch、AvatarCrop、TagManager 的测试明确区分关闭与退出完成。`test/DialogTestRoot.tsx` 为 jsdom 提供显式完成信号，不把 `open=false` 当作自动卸载。

`e2e/dialog-lifecycle.spec.ts` 检查真实 CSS 退出期间内容与 URL 稳定、退出后销毁、嵌套焦点及 reduced-motion/移动端重开；`e2e/admin-content-access.spec.ts` 覆盖详情原位切换、frame 宽度与版本历史叠加行为。修改本契约后运行 full Vitest、`bun run check`、生产构建和对应 Playwright。
