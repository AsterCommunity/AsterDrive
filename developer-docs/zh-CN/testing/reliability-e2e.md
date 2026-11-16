# 持久化工作流崩溃恢复验收

该验收启动真实 AsterDrive 子进程，在已确认的持久化检查点从测试控制器发送 SIGKILL，再用同一数据库、对象目录、Redis（PostgreSQL 模式）和上传目录重启。适用于 Linux/Unix：进程检查点通过 Unix domain socket 通知；测试不以 sleep 或超时猜测检查点。

```bash
make reliability-e2e
make reliability-e2e SCENARIO=upload CHECKPOINT=upload.after_blob_persist_before_finalize_transaction SEED=12345
make reliability-e2e SCENARIO=archive-task SEED=12345
make reliability-e2e BACKEND=postgres SEED=12345
ASTER_RELIABILITY_STORAGE=minio make reliability-e2e BACKEND=postgres SCENARIO=archive-task SEED=12345
```

`SCENARIO` 支持 `all`、`upload` 和 `archive-task`。默认 SQLite + 本地文件；每个场景创建隔离数据库及临时目录。设置 `ASTER_TEST_DATABASE_BACKEND=postgres` 时使用真实 PostgreSQL 和 Redis。`SEED` 通过固定 SplitMix64 映射到输入字节，失败产物的 `replay.json` 记录 seed、checkpoint 和构建 revision。

六个稳定检查点位于上传 staging sync、blob 持久化、上传最终提交后清理，以及 archive task claim、结果副作用、终态更新边界。上传恢复测试检查成功下载内容、session 重试幂等、文件/blob 引用计数、用户 quota、审计和本地对象归属。后台任务在租约到期后检查 token fencing、唯一 archive 结果和独立请求可用性。

失败时 `target/reliability/<seed>-<checkpoint>-<run>/` 保存启动代次、检查点确认、SIGKILL 和前后结构化快照。子进程日志在落盘前移除 URL、凭据和标记为敏感的行。缺失、重复、超时或旧代次通知都会使测试失败；重放时使用相同 `SCENARIO`、`CHECKPOINT`、`SEED`。

offset-staging 在组装租约过期后，还必须取得 staging inode 的独占文件锁，才能用观察到的 `updated_at` 做 CAS 恢复。blob 发布保留 staging；非去重模式使用 session 专属稳定 key，禁止覆盖已有对象。最终事务已提交的 retry 只返回原文件并清理 staging，不重复计费。数据库提交结果不明确时，继续沿现有错误模型保留可恢复状态。

MinIO 模式把 archive 输出路由到 S3，源文件仍使用本地 offset-staging。它只接受三个 `task.*` 检查点；三个 `upload.*` 检查点专门验证本地格式，选择不适用的组合会失败。任务的非去重输出和去重私有候选均在外部写入前登记零引用 blob，发布事务原子移交候选对象所有权；崩溃遗留的对象必须有 `cleanup_pending_since`，由现有 blob GC 清理。恢复后只允许一个有产品引用的 archive 对象。MySQL 真实进程矩阵属于后续范围。

MinIO 官方分发已停止提供；上述 Make 入口首次运行会用 `scripts/reliability/minio.Dockerfile` 从官方固定 commit `07c3a429bfed433e49018cb0f78a52145d4bedeb` 构建本地测试镜像，并校验源码归档 SHA-256。需要联网下载 Go 构建依赖，后续运行复用镜像。

测试 fixture 专用的故障开关只在 `reliability-e2e` feature 下编译，且只有 fixture 会安装 IPC controller；普通 server 不读取故障环境变量。正式 release 不启用此 feature。独立可靠性 CI 串行运行 SQLite/local-filesystem 和 PostgreSQL/local-filesystem 六个检查点；scheduled/manual job 用 PostgreSQL/MinIO 运行所有适用检查点，并记录及重复三个 seed。controller、确定性 seed 和诊断脱敏的自测不需要启动完整产品场景。
