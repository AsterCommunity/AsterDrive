# Issue 718：迁移栈调查（未完成）

Windows 复现基线为上游 `master` 的 `41467b7e74ac8bc23b7c0ff87938a7dc70a9d733`；候选修复基于同一 revision。以下结果不代表修复已在 Windows 上验证。

## 可重复的探针

`aster_drive_migration` 的 `tests::migration_stack_budget_probe` 在指定栈大小的线程中创建 current-thread Tokio runtime，使用单连接、全新内存 SQLite 执行现有完整迁移。它测量的是这个测试工作负载能否在指定的线程栈预算内完成，并非服务主线程的精确峰值，也不能代替 Windows 原始可执行文件的复现。操作系统可能调整线程实际栈大小。

每次将探针作为独立进程运行：栈溢出可能直接终止进程。Windows PowerShell 示例（先在修复前基线运行，保留源码、toolchain、系统和命令记录）：

```powershell
$env:ASTER_MIGRATION_PROBE_STACK_KIB = '1024'
cargo test --locked -p aster_drive_migration --lib tests::migration_stack_budget_probe -- --ignored --exact --nocapture
```

更改预算并分别重复执行；以 `cargo test --release` 在相同条件下进行 Release 对照。记录退出码及日志，不能把二分得到的最小成功预算称为精确栈高水位或最坏情况保证。该探针创建 runtime 的方式也不同于拟议的共享调用方 runtime 的 worker，结果不能直接证明后者的栈预算。

## 本机结果与缺口

Windows 11 x64、Rust 1.99.1、MSVC target 上，Debug 原始程序在 baseline 迁移时报告主线程栈溢出并以 `0xC00000FD` 退出；相同源码及工具链下 Release 达到 readiness。两个原始程序 PE 栈预留均为 1 MiB。独立探针每个预算运行一次：Debug 的 832 KiB 失败、864 KiB 和 8192 KiB 成功；Release 的 192 KiB 失败、208 KiB 成功。未采集崩溃调用栈；前端构建使用 Node 24.22.1，低于仓库要求，但两种程序使用相同资源。

暂定 worker 请求 8 MiB：该值在 Windows 探针中成功，且为本次 Debug 成功请求值 864 KiB 的约 9.5 倍；不同入口和未来变更可能提高消耗，仍需在 Windows 上运行实际修复后的 Debug 服务和升级测试，不能把探针成功视为充分证明。Windows 使用的探针是独立临时集成测试，和这里的探针入口并不相同。

调用方取消只会停止等待，不会停止 worker；原 runtime 必须继续存活和驱动。runtime 停止、进程退出或 abort 型 panic 不能保证迁移完成。迁移回调内的 unwind 会转为错误以允许 Forge 回滚及释放锁，恢复 SQLite 外键设置后再传播 panic；这些保证不涵盖所有 runtime 或进程级故障。

在 macOS ARM64 的候选实现上，迁移 crate 单元测试、默认功能的 SQLite 服务首次启动与重启、SQLite 升级和失败回滚测试通过。使用本地 Docker 的 PostgreSQL 16，单连接池全量迁移、已有数据库升级和失败后重试均通过；MySQL 8.4 的已有数据库升级、失败后重试和并发迁移测试均通过。这些是本地结果，不代表 Windows 修复后验证已完成。PostgreSQL/MySQL 的 panic 注入和 runtime 关闭场景尚未验证。
