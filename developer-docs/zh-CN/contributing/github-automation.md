# GitHub 自动化

AsterDrive 使用默认分支中的受信任脚本维护 PR 元数据、关联 Issue 生命周期、聚合 CI 诊断和默认分支故障 Issue。自动化只消费 GitHub API 数据，不 checkout 或执行外部 PR 分支代码。

## PR 元数据

`.github/workflows/pr-automation.yml` 监听 `pull_request_target`，根据 changed files 确定性维护以下信息：

- 语言与文档标签：`Rust`、`TypeScript`、`Documentation`、`Dependencies`
- 产品范围标签：现有 `Scope:*`
- 高风险边界：migration、认证、上传、锁、配额、WebDAV、WOPI、internal storage、部署和 workflow 变更使用 `Risk: High`

自动化只维护可由文件路径证明的标签。`Priority:*` 仅在所有 closing issue 给出同一个优先级时继承；冲突或缺失时不猜测。脚本保留人工添加的非托管标签。

PR 使用 GitHub 原生 closing keyword（例如 `Fixes #123`）关联 Issue 后，自动化会为 Issue 增加 `Wait For PR`，并在没有其他状态时增加 `Status: In Progress`。合并或关闭后，在不存在另一个 open closing PR 时移除这两个临时状态；成功合并还会给 PR 增加 `Merged`。Issue 是否关闭仍由 GitHub 原生 closing 语义决定。

## CI 聚合与 PR Gate

`.github/workflows/ci-diagnostics.yml` 在各 CI workflow 完成后重新读取 PR 最新 HEAD 的检查状态：

1. 根据与现有 workflow 相同的 path filter 计算本次需要运行的检查。
2. 将结果汇总为稳定的 `PR Gate` Check Run。
3. 相关 CI 仍在运行时添加 `CI: Running`；至少一个相关 workflow 全部成功时切换为 `CI: Passed`。失败、取消、新 commit 或 PR 关闭/合并会清理这两个 CI 生命周期标签；没有相关 CI 的元数据变更不添加 `CI: Passed`。
4. 失败时创建一条包含隐藏 marker 的 PR 评论；后续运行只更新该评论。
5. 新 commit 到达后只处理当前 HEAD，旧 SHA 结果不会覆盖新诊断。
6. 所有必要检查恢复后，评论更新为 resolved。

`PR Gate` 只汇总已有 CI，不重新执行测试。添加或修改 path-filtered workflow 时，必须同步更新 `scripts/github/automation-config.mjs` 的 `PR_WORKFLOWS`，并补路径边界测试。

PR 打开或更新时，PR automation 会立即为当前 HEAD 创建初始 Gate。没有任何 path-filtered CI 的变更直接成功；其余变更保持 pending，直到 CI diagnostics 根据 workflow 完成事件更新。`Repository Automation` workflow 负责验证这些脚本、单元测试和所有 Actions YAML，防止自动化自身成为盲区。

历史 workflow 已结束但 Gate 未收敛时，可从 Actions 页面手动运行 `CI Diagnostics`，并传入源 workflow 的 run ID。该入口只重新读取 GitHub API 状态并更新 Gate、标签和诊断评论，不重新执行源 workflow。

PR 已关闭或合并但 Gate 仍停留在 pending 时，可手动运行 `PR Automation`，传入 `pull_request_number`。该入口在默认分支受信任代码中以 `cancelled` 终结未完成 Gate，并按关闭 PR 的最终状态清理生命周期标签；它使用 Actions token 执行 Check Run 更新，个人 token 不能直接 PATCH Actions 创建的 Check Run。

## 默认分支与定时任务故障

默认分支或 scheduled workflow 失败时，自动化使用 `workflow + branch + failed job set` 生成稳定 fingerprint：

- 首次失败创建带 `CI: Failure` 的 Issue。
- 同一 fingerprint 再次失败更新原 Issue 和出现次数。
- runner、Docker、网络、磁盘或超时信号同时增加 `CI: Infrastructure`。
- 同一 workflow 与 branch 连续成功两次后关闭仍打开的故障 Issue。

故障 Issue 的分类是诊断入口，不代表已经证明根因。完整证据仍以链接的 workflow、job 和 artifact 为准。

## 权限和安全边界

- `pull_request_target` 和 `workflow_run` 都只 checkout 默认分支。
- checkout action 固定 commit SHA，且 `persist-credentials: false`。
- PR workflow 只获得 issue / pull request 写权限。
- CI workflow 只额外获得 actions 读和 checks 写权限。
- 标题、正文、文件路径、日志、job 名称及外部链接均按不可信输入处理，不拼接为 shell 命令。
- 模型分析不参与合并门禁、产品优先级、Issue 自动关闭或发布决策。

## 本地验证

核心规则使用 Node.js 内建 test runner：

```bash
node --test scripts/github/*.test.mjs
node scripts/github/check-actionlint.mjs
```

修改 workflow 后还应运行 YAML / Actions 静态检查以及：

```bash
git diff --check
```
