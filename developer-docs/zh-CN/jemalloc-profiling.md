# Jemalloc 堆画像

本文记录 AsterDrive 维护者排查 Rust heap 增长时的 jemalloc profiling 跑法。它面向开发和问题定位，不是普通部署必开的生产配置。

## 适用场景

优先用它处理下面这类问题：

- `Private_Dirty` / `Anonymous` 在长稳测试里持续净增长
- `/health` 里的 allocator 指标随请求增长后长期不回落
- 压测结束后 RSS 没有明显下降，需要确认是 Rust heap、allocator 缓存、mmap、线程栈还是其他来源
- 需要知道“哪条调用栈正在持有 heap”，而不是继续只看 `/proc` 汇总数字

如果 1 到 2 小时内 `Private_Dirty` / `Anonymous` 基本稳定，先不要急着打开 heap profiling。jemalloc profiling 有额外开销，而且会持续写 `.heap` 文件。

## 现有接线

仓库已经有基础接线：

- `Cargo.toml` 里有 `jemalloc`、`jemalloc-stats`、`jemalloc-profiling`
- `jemalloc-profiling` 会启用 `tikv-jemallocator/profiling`，并包含 `jemalloc-stats`
- `src/main.rs` 在 `jemalloc` feature 下把 `tikv_jemallocator::Jemalloc` 设为全局 allocator
- `src/main.rs` 还通过 `_rjem_malloc_conf` 内置了低内存默认项：`narenas:1`、`dirty_decay_ms:1000`、`muzzy_decay_ms:1000`、`background_thread:true`
- `aster_forge_alloc::stats()` 在 `jemalloc-stats` 下会读取 `stats::allocated` 和 `stats::resident`

所以通常不需要改代码。第一轮排查直接编 profiling binary，再用运行时配置打开 profile dump。

## 编译 profiling binary

推荐用仓库已有的 `profiling` profile。它接近 release，但保留符号并关闭 LTO，方便 `jeprof` 展开栈。

```bash
RUSTFLAGS="-C force-frame-pointers=yes" \
cargo build --profile profiling --features jemalloc-profiling
```

产物路径：

```bash
target/profiling/aster_drive
```

线上排查时建议单独命名，不要覆盖当前稳定 binary：

```bash
cp target/profiling/aster_drive ./aster_drive-jprof
```

如果要把结果拿回开发机分析，保留同一个 binary。`jeprof` 需要用生成 heap profile 的那份 binary 做符号解析。

## 启动时打开 profiling

先准备 dump 目录：

```bash
sudo mkdir -p /var/log/asterdrive/jemalloc
sudo chown -R asterdrive:asterdrive /var/log/asterdrive/jemalloc
```

AsterDrive 当前使用 `tikv-jemallocator` 的默认前缀构建，jemalloc 运行时配置环境变量通常是 `_RJEM_MALLOC_CONF`，不是裸 `MALLOC_CONF`。如果你不确定部署包是否改成了 unprefixed 构建，可以两个都设成同一个值。

推荐低开销起步配置：

```bash
export ASTER_JEMALLOC_PROF='prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true'

_RJEM_MALLOC_CONF="$ASTER_JEMALLOC_PROF" \
MALLOC_CONF="$ASTER_JEMALLOC_PROF" \
./aster_drive-jprof serve
```

参数含义：

- `prof:true`：启用 heap profiling。必须使用 `--features jemalloc-profiling` 编译出的 binary。
- `prof_active:true`：进程启动后马上开始采样。
- `prof_prefix:...`：heap dump 文件前缀。
- `lg_prof_sample:19`：平均每 `2^19 = 512 KiB` 分配采样一次。几十 MiB 级别的服务先用这个。
- `lg_prof_interval:26`：累计分配约 `2^26 = 64 MiB` 时自动 dump 一份 profile。低流量服务用这个比 `28` 更容易拿到样本。
- `prof_final:true`：进程退出时再 dump 一份最终 profile。

如果样本太稀，改成：

```text
lg_prof_sample:18
```

这会变成约 `256 KiB` 一次采样，结果更细，开销也更高。

如果 dump 文件太多，改大：

```text
lg_prof_interval:28
```

这会变成约 `256 MiB` 累计分配 dump 一次。

## systemd 方式

如果服务由 systemd 托管，使用 override：

```bash
sudo systemctl edit asterdrive
```

写入：

```ini
[Service]
Environment="ASTER_JEMALLOC_PROF=prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true"
Environment="_RJEM_MALLOC_CONF=prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true"
Environment="MALLOC_CONF=prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true"
```

然后：

```bash
sudo mkdir -p /var/log/asterdrive/jemalloc
sudo chown -R asterdrive:asterdrive /var/log/asterdrive/jemalloc
sudo systemctl daemon-reload
sudo systemctl restart asterdrive
```

确认是否生效：

```bash
sudo journalctl -u asterdrive -n 200 --no-pager
sudo ls -lh /var/log/asterdrive/jemalloc
```

如果 journal 里出现 `<jemalloc>:` unknown option 或 malformed conf，先修配置字符串。不要在配置里加空格。

## 推荐采样流程

排查疑似泄漏时，按这个顺序跑：

1. 用 profiling binary 启动服务。
2. 等待服务启动完成并空闲 3 到 5 分钟。
3. 保留最早的一份 `.heap` 作为 baseline。
4. 跑真实流量、k6 soak，或直接放置 12 到 24 小时。
5. 停服务，让 `prof_final:true` 生成最终 profile。
6. 用 `jeprof --base baseline final` 看净增长调用栈。

关键是看 diff，不是只看最后一份 profile。单份 profile 只能说明“当前谁占 heap”，diff 才能说明“这段时间谁净增长”。

## 使用 jeprof 分析

先确认有 `jeprof`：

```bash
which jeprof
```

如果系统没有，需要安装 jemalloc 工具包，或者从对应 jemalloc 构建里带出 `jeprof`。不同发行版包名不完全一样，例如 Debian / Ubuntu 上可能是 `jemalloc-bin`。

查看单份 profile：

```bash
jeprof --text ./aster_drive-jprof \
  /var/log/asterdrive/jemalloc/asterdrive.<pid>.<seq>.heap \
  | head -80
```

比较 baseline 和最终 profile：

```bash
jeprof --text \
  --base /var/log/asterdrive/jemalloc/asterdrive.<pid>.0001.i0001.heap \
  ./aster_drive-jprof \
  /var/log/asterdrive/jemalloc/asterdrive.<pid>.0010.f.heap \
  | head -80
```

也可以导出图形格式：

```bash
jeprof --svg ./aster_drive-jprof \
  /var/log/asterdrive/jemalloc/asterdrive.<pid>.<seq>.heap \
  > /tmp/asterdrive-heap.svg
```

## 结果解读

常见判断：

- `/proc` 稳定，`jeprof --base` 也没有明显净增长：基本不像泄漏。
- `/proc` 持续增长，`jeprof --base` 指向具体 Rust 栈：优先查对应业务缓存、任务队列、buffer、连接池或没有释放的对象。
- `/proc` 增长，但 `jeprof` 看不到明显增长：优先查非 Rust heap 来源，例如 mmap、文件映射、线程栈、C 库内部分配或内核页缓存口径。
- `allocated` 下降但 `resident` 不马上下降：可能是 allocator 保留页或 decay 行为，不等价于泄漏。
- `Pss_File` 变化而 `Anonymous` 不涨：通常不是 Rust heap 泄漏的主线索。

## 排障清单

没有生成 `.heap` 文件时，按顺序查：

1. binary 是否真的是 `cargo build --profile profiling --features jemalloc-profiling` 产物。
2. systemd / shell 是否真的把 `_RJEM_MALLOC_CONF` 传进进程。
3. `prof_prefix` 所在目录是否存在，且服务用户可写。
4. `lg_prof_interval` 是否太大，低流量下还没触发自动 dump。
5. journal / stderr 是否有 `<jemalloc>:` 配置错误。
6. 是否把配置写成了带空格的字符串，例如 `prof:true, prof_active:true`。不要这样写。

栈只有地址或符号很差时，按顺序查：

1. 是否用同一份 `aster_drive-jprof` 分析对应 `.heap`。
2. binary 是否来自 `profile.profiling`，而不是 stripped release binary。
3. 编译时是否带了 `RUSTFLAGS="-C force-frame-pointers=yes"`。
4. 是否在另一台机器分析，导致路径、debug info 或 build id 对不上。

## 后续可改进项

如果后续需要精确控制 dump 时点，可以加一个仅在 `jemalloc-profiling` feature 下可用的内部诊断入口，例如：

```text
POST /api/v1/admin/diagnostics/jemalloc/dump
```

或 CLI：

```bash
aster_drive diagnostics jemalloc-dump
```

实现上应调用 `mallctl("prof.dump")`。这个能力必须 feature gate，且只能给管理员或本机运维入口使用。不要默认暴露给公网路径，否则它会变成一个可被滥用的磁盘写入和性能压力开关。
