# Issue #486 Future 体积与分配证据

> 状态：历史快照。本文记录基线 revision `9b55dbfd18892e09227bfb0ff5de950586071dd5`
> 与 issue #486 实现分支在 2026-08-08 的聚焦测量，不作为当前实现规范。当前行为以相关源码、测试和
> `clippy::large_futures` 输出为准。

## 测量环境

- `rustc 1.97.1 (8bab26f4f 2026-07-14)`，LLVM `22.1.6`
- `cargo 1.97.1 (c980f4866 2026-06-30)`
- `x86_64-unknown-linux-gnu`，Linux `6.12.85+deb13-amd64`
- 基线与改后 checkout 共享同一 `target`，分别重新编译 AsterDrive crate
- Future 体积来自同一条全 feature Clippy 命令；归档 copy helper 的精确体积来自
  `std::mem::size_of_val`
- 分配数据由线程局部启用的 `System` counting allocator 记录；输入、状态构造、认证 warm-up 和
  5 MiB 上传 `Bytes` 均在测量区间外创建

## Future 体积

默认 16 KiB 阈值命令：

```bash
RUSTC_WRAPPER= cargo clippy -p aster_drive --lib --all-features \
  --message-format=short -- \
  -W clippy::unused_async \
  -W clippy::large_futures
```

| 边界 | 基线 | 改后 | 结论 |
| --- | ---: | ---: | --- |
| 归档预览 copy helper | 65,792 B | 272 B | 内联 64 KiB 数组已移出状态机 |
| 归档解包 copy helper | 65,792 B | 272 B | 内联 64 KiB 数组已移出状态机 |
| 解包任务处理 Future | 67,448 B | 10,712 B | 改后值由临时 8 KiB 阈值复核 |
| 解包 task-spec 上层 Future | 67,736 B | 11,000 B | 不再触发默认阈值 |
| 预览任务处理 Future | 67,312 B | < 8,192 B | 改后在临时 8 KiB 阈值下也未报告 |
| 预览 task-spec 上层 Future | 67,600 B | < 8,192 B | 改后在临时 8 KiB 阈值下也未报告 |
| 个人上传 route | 23,616 B | 23,584 B | 同步化 helper 后减少 32 B，无装箱 |
| 团队上传 route | 23,624 B | 23,592 B | 同步化 helper 后减少 32 B，无装箱 |
| 上传 service 六个入口 | 16,432/23,264 B | 16,400/23,232 B | 每个减少 32 B，无装箱 |
| WebDAV dispatch | 27,280 B | 27,280 B | 未改变 dispatch 结构 |

临时 8 KiB 配置只用于取得归档上层改后数值，没有修改仓库阈值。该检查仍显示其他 8–16 KiB
Future，因此不适合作为通用 CI 门槛。

## 分配次数与字节数

归档 copy 测量使用空的精确长度输入和 sink writer，只覆盖 Future 首次 poll 到完成。上传测量使用
预先构造的 5 MiB `Bytes` 执行第一个 offset-staging chunk。WebDAV 测量先 warm-up 认证缓存，再执行
一次完整的 authenticated `OPTIONS /webdav/` dispatch。

| 操作 | 基线分配 | 改后分配 | 差值 |
| --- | ---: | ---: | ---: |
| 归档预览 copy | 0 次 / 0 B | 1 次 / 65,536 B | +1 次 / +65,536 B |
| 归档解包 copy | 0 次 / 0 B | 1 次 / 65,536 B | +1 次 / +65,536 B |
| 5 MiB chunk upload | 346 次 / 2,184,661 B | 346 次 / 2,184,661 B | 0 次 / 0 B |
| WebDAV authenticated OPTIONS | 73 次 / 34,233 B | 73 次 / 34,233 B | 0 次 / 0 B |

归档新增分配就是每次 copy 的一个固定 64 KiB `Box<[u8]>`，完整循环复用该缓冲区。上传和 WebDAV
没有新增每请求分配，因而没有引入 `Box::pin`、动态 Future dispatch 或其他热路径分配边界。

## Lint 分类

默认阈值下，主 crate 的 `large_futures` 从 17 条降至 9 条：

- 归档 8 条级联警告全部消失；
- 余下 8 条属于 upload route/service，1 条属于 WebDAV dispatch；
- 四条 migration 警告保持不变，仍按顺序执行的 migration 路径单独处理。

五个生产 helper 的 `unused_async` 已清理。余下三条均为
`src/services/auth/local/tokens/refresh.rs` 的 debug/test-support hook：一次 contention 通知以及 test hook
的安装、清理入口。相邻的 pause hook 确实等待通知且未触发该 lint；这些入口均不属于本次生产路径清理。

## 回归边界

两个 archive copy helper 的单元测试均覆盖：

- 精确长度及内容保持；
- 输入提前结束；
- 输入超过声明长度；
- shutdown/cancellation；
- writer 失败传播；
- Future 小于 16 KiB；
- 每次 copy 恰好一次 65,536 B 缓冲区分配。
