# WebDAV 合规与兼容性检查

本文说明如何验证 AsterDrive 的 WebDAV 协议行为。检查分为三层：

1. Rust 回归测试，验证项目内部已经明确的协议边界；
2. Litmus 合规基线，验证一组稳定、可重复比较的 WebDAV 协议用例；
3. rclone、curl、cadaver 真实客户端测试，验证客户端实际工作流。

三层检查解决的问题不同。单元或集成测试通过，不代表外部客户端一定兼容；Litmus 全部通过，也不等于覆盖了 AsterDrive 的存储、配额、版本、审计和团队工作空间语义。

## 当前测试入口

| 层级 | 入口 | 默认执行 | 用途 |
| --- | --- | --- | --- |
| WebDAV Rust 回归 | `tests/webdav/protocol.rs` 等模块 | 是 | 精确验证方法、状态码、请求头、锁、属性、Range、路径等边界 |
| Litmus 合规基线 | `tests/webdav/litmus_compliance.rs` | 否，外部用例标记为 `ignored` | 用固定 Litmus 版本执行 `basic`、`copymove`、`props`、`locks`、`http` |
| 真实客户端兼容性 | `tests/webdav/client_e2e.rs` | 否，标记为 `ignored` | 用固定版本的 rclone、curl、cadaver 执行真实客户端流程 |
| CI 工作流 | `.github/workflows/webdav-compatibility.yml` | 按路径、定时或手动触发 | 固化工具版本、运行检查并保存产物 |

所有 WebDAV 集成测试、外部客户端测试、Litmus 测试及其 fixture 都集中在 `tests/webdav/`。Cargo 通过 `tests/webdav/main.rs` 自动发现统一的 `webdav` 测试 target，不需要在 `Cargo.toml` 为每个模块单独声明 `[[test]]`。

协议实现的主要定位入口是：

- `src/webdav/protocol.rs`：`Depth`、`Destination`、`If`、ETag 等协议头；
- `src/webdav/responses.rs`：HTTP 状态码和 XML 响应；
- `src/webdav/props/`：`PROPFIND`、`PROPPATCH`；
- `src/webdav/transfer/`：`GET`、`HEAD`、`PUT`；
- `src/webdav/resources/`：`MKCOL`、`DELETE`、`COPY`、`MOVE`；
- `src/webdav/locks/` 和 `src/webdav/db_lock_system.rs`：`LOCK`、`UNLOCK` 和锁持久化；
- `src/webdav/fs/`、`src/webdav/file/`、`src/webdav/path_resolver.rs`：文件系统适配和路径解析。

## 先跑项目内 WebDAV 回归

修改 WebDAV 后先跑最小目标，不要一上来编译整个测试矩阵：

```bash
cargo test --test webdav protocol::<test_name> -- --nocapture
```

例如修改 `MKCOL`：

```bash
cargo test --test webdav protocol::test_webdav_mkcol -- --nocapture
```

修改范围覆盖多个 WebDAV 模块后，再跑完整目标：

```bash
cargo test --test webdav -- --nocapture
```

这一步适合验证精确的 AsterDrive 行为，也方便为 Litmus 暴露的问题补边界测试。修复合规问题时，不能只删 Litmus 基线项；应先补一个不依赖外部二进制的 Rust 回归测试，再更新合规基线。

## Litmus 0.18 固定基线

当前自动化检查固定为 **Litmus 0.18**。Litmus、内嵌 neon 以及真实客户端工具的版本、源码提交和 SHA-256 均集中记录在：

```text
scripts/ci/webdav-compat/versions.env
```

测试代码也在 `tests/webdav/litmus_compliance.rs` 中固定了版本、分组和预期用例数：

| 分组 | 预期用例数 |
| --- | ---: |
| `basic` | 16 |
| `copymove` | 13 |
| `props` | 33 |
| `locks` | 40 |
| `http` | 4 |

这五组是 Litmus 0.18 的默认套件，也是普通 PR 的合规门禁。固定版本、源码提交和校验和，可以让用例名称、数量、输出格式和已知差异保持稳定；不能只替换 `LITMUS_BIN` 后仍按当前基线解释其他版本的结果。升级基线时必须同步更新版本、用例数、解析验证、CI 安装方式和 known-difference 文件。

Litmus 0.18 安装后还包含两类不进入普通 PR 门禁的可选套件。

资源与压力套件：

- `largefile`：大文件传输，包含约 2 GiB 的资源；
- `lockbomb`：多线程高强度 LOCK/UNLOCK 压力；
- `lockbomb-single`：单线程高次数 LOCK/UNLOCK 压力。

安全实现策略套件：

- `protected`：检查服务器保留元数据路径，固定使用 `TEST_PROTECTED=.DAV`，共 25 个用例。

`largefile` 和锁压力套件应放在定时或手动 job 中，并使用独立的超时与资源配额。`protected` 针对 Apache `mod_dav_fs` 等会把可信属性数据库放在 WebDAV 路径附近的实现，不属于 RFC 4918 合规组。AsterDrive 的 dead properties 存在数据库中，当前也没有服务器专用 `.DAV` 命名空间，因此这组测试作为显式的安全策略探针运行，其结果不进入默认合规 baseline。

> **AsterDrive 适用性说明：** AsterDrive 的 dead properties 存放在数据库 `entity_properties` 表中，并通过解析后的 file/folder `entity_type + entity_id` 关联；它没有把 `.DAV` 或其他目录名映射为内部属性数据库，也没有通过 WebDAV 文件路径暴露 property records。产品语义上，`.DAV` 是合法的普通用户目录，AsterDrive 不需要为了这组 Litmus 用例保留或封禁该名称。用户创建、读取、移动或删除 `.DAV` 只会操作普通 file/folder entity，不会触达属性表、锁表或其他可信内部元数据。因此，`protected` 报告的 15 项差异是与 Apache 文件式属性数据库模型的预期差异，本身不代表 dead-property storage 暴露或产品安全漏洞。

`webdav_block_system_files_enabled` 和 `webdav_block_system_file_patterns` 仍然只是面向 `.DS_Store`、`Thumbs.db` 等客户端垃圾文件的可配置产品策略，不承担内部元数据隔离职责，也不构成 WebDAV 路径安全边界。AsterDrive 的属性隔离边界是 path resolver、workspace scope、实体权限以及 `entity_properties` repository，而不是某个保留目录名称。

## 安装固定的 Litmus 0.18

安装脚本、客户端安装脚本和版本清单集中放在 `scripts/ci/webdav-compat/`，不要把下载源码或构建产物放进仓库。脚本会把 Litmus 和 neon 下载到临时目录，校验 SHA-256，从固定提交构建，并安装到 `WEBDAV_COMPAT_TOOLS_DIR`。

macOS 先安装构建依赖：

```bash
brew install autoconf automake pkg-config openssl@3
```

然后从 AsterDrive 仓库根目录运行：

```bash
WEBDAV_COMPAT_TOOLS_DIR="$HOME/.local/webdav-compat" \
  scripts/ci/webdav-compat/install-litmus.sh

"$HOME/.local/webdav-compat/bin/litmus" --version
```

Linux 需要 `autoconf`、`automake`、C 编译工具链、`curl`、`libexpat` 开发文件、OpenSSL 开发文件和 `pkg-config`。安装脚本在 Linux 和 macOS 上共用，CI 也调用同一个入口。

Ubuntu 24.04 的 `apt` 仓库提供的是 Litmus 0.13，可用于临时手工探测，但不符合当前 0.18 固定基线。CI 因此不安装 `litmus` apt 包，而是调用上述脚本从已校验的固定源码提交构建。

期望输出：

```text
litmus 0.18
```

使用安装目录下的 `bin/litmus` wrapper。它已经记录测试程序、`htdocs` 和内嵌 neon 的安装位置，测试进程切换工作目录后仍能正常运行。

## 本地运行 Litmus 合规检查

回到 AsterDrive 仓库根目录。建议显式设置绝对路径和产物目录：

```bash
mkdir -p "$PWD/artifacts/webdav-local"

LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav litmus_compliance::test_litmus_ -- \
  --ignored \
  --skip resource_litmus:: \
  --skip security_policy_litmus:: \
  --nocapture --test-threads=1
```

`--test-threads=1` 是固定要求。每个分组都会启动独立的本地 HTTP 服务、创建临时 WebDAV 用户和账号，并使用自己的工作目录；串行执行可以让输出和产物归属保持清楚。

只复现一个分组时，把测试名放在 `--` 之前：

```bash
LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav litmus_compliance::test_litmus_basic -- \
  --ignored --nocapture --test-threads=1
```

可用测试名：

```text
litmus_compliance::test_litmus_basic
litmus_compliance::test_litmus_copymove
litmus_compliance::test_litmus_props
litmus_compliance::test_litmus_locks
litmus_compliance::test_litmus_http
```

资源密集型套件单独放在 `tests/webdav/litmus/resource.rs`：

```text
litmus_compliance::resource_litmus::test_litmus_largefile
litmus_compliance::resource_litmus::test_litmus_lockbomb
litmus_compliance::resource_litmus::test_litmus_lockbomb_single
```

只运行 `largefile`：

```bash
LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav \
  litmus_compliance::resource_litmus::test_litmus_largefile -- \
  --ignored --nocapture --test-threads=1
```

三个资源密集型套件一起运行时，可以把过滤器缩短为 `litmus_compliance::resource_litmus::`。`largefile` 传输约 2 GiB，超时为 30 分钟；`lockbomb` 使用 20 个线程、每个线程执行 20,000 次 LOCK/UNLOCK，超时为 2 小时；`lockbomb-single` 单线程执行 20,000 次，超时为 1 小时。运行前应为临时存储、数据库和产物目录预留足够空间，并继续保持 `--test-threads=1`。

`protected` 安全策略探针放在 `tests/webdav/litmus/security_policy.rs`，固定 25 个用例和 `TEST_PROTECTED=.DAV`，避免调用方环境静默改变被测命名空间：

```bash
LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav \
  litmus_compliance::security_policy_litmus::test_litmus_protected -- \
  --ignored --nocapture --test-threads=1
```

该命令用于观察当前架构与“服务器保留 `.DAV` 元数据目录”模型的差异。当前产品决定是不建立服务器专用 `.DAV` 目录，也不为这 15 项模型差异创建 RFC known-difference baseline；普通系统垃圾文件名的写入拦截继续由 AsterDrive 原生协议测试覆盖。若未来属性存储架构发生变化，再重新审查该决定。

这里的 `FAIL` 应解读为 Litmus 预设的 `.DAV` 保留路径策略与 AsterDrive 产品模型不同，而不是 WebDAV 请求已经访问到内部 dead-property storage。判断真正的安全回归时，应验证 WebDAV path resolver 是否越过 file/folder entity 边界并触达内部属性、锁或凭据存储，而不是只看 `.DAV` 这个名称是否可创建。

安全策略探针仍严格检查进程状态一致性、超时和 25 个用例的数量，但会把 Litmus 的 `FAIL` / `WARNING` 记录到 `result.json` 的 `observed_differences`，而不是按默认合规 baseline 判定 Rust 测试失败。这样既保留原始安全信号，也不会把不适用的 Apache `.DAV` 存储模型伪装成 AsterDrive 的 RFC 差异。

如果没有设置 `LITMUS_BIN`，测试会从 `PATH` 查找 `litmus`。为了避免误用其他版本，本地合规检查仍建议显式指定绝对路径。

## Litmus 测试实际做了什么

`tests/webdav/litmus_compliance.rs` 不要求手动启动 AsterDrive：

1. 通过 `common::setup()` 创建隔离的测试状态和数据库；
2. 创建随机用户以及独立的 WebDAV Basic Auth 账号；
3. 在 `127.0.0.1` 的随机端口启动真实 Actix HTTP 服务；
4. 将挂载地址设为该服务的 `/webdav/`；
5. 通过 `TESTS=<group>` 每次只运行一个 Litmus 分组；
6. 单个分组最多运行 120 秒，超时后终止整个 Litmus 进程组；
7. 停止 HTTP 服务，解析 Litmus 输出并与 committed baseline 比较；
8. 写入结构化报告，并对落盘日志中的用户名、密码和 Basic Auth 值做脱敏。

这条链路走真实 HTTP 和 WebDAV Basic Auth，不是直接调用 handler 的内存测试。

## 产物和排查顺序

设置 `ASTER_WEBDAV_COMPAT_ARTIFACT_DIR` 后，每个分组使用独立目录：

```text
artifacts/webdav-local/litmus/basic/
artifacts/webdav-local/litmus/copymove/
artifacts/webdav-local/litmus/props/
artifacts/webdav-local/litmus/locks/
artifacts/webdav-local/litmus/http/
```

重点文件：

| 文件 | 内容 |
| --- | --- |
| `result.json` | 结构化用例结果、已接受差异和判定错误 |
| `stdout.log` | Litmus 标准输出 |
| `stderr.log` | Litmus 标准错误 |
| `debug.log` | neon HTTP 请求和响应调试轨迹 |
| `child.log` | Litmus 子进程日志，存在时一并留档 |

建议按下面的顺序看：

1. 先看 `result.json` 的 `errors`、`observed_differences`、`accepted_differences` 和失败用例名称；
2. 再看 `stdout.log`，确认 Litmus 报出的期望状态和实际状态；
3. 最后用 `debug.log` 对照请求方法、URI、WebDAV header、响应状态码和 XML body；
4. 如果测试超时或提前退出，再看 `stderr.log`、`child.log`。

测试会脱敏自动生成的凭据，但产物仍可能包含文件名、路径、响应体和部署信息。CI 产物只应按当前仓库权限和保留策略保存。

## 如何解释 Litmus 状态

| 状态 | 含义 | 基线处理 |
| --- | --- | --- |
| `pass` | 用例通过 | 不写入 known-difference |
| `FAIL` | 协议结果不符合用例预期 | Baseline 组先修复；Probe 组记录在 `observed_differences`，先判断是否属于产品模型差异 |
| `SKIPPED` | 前置能力缺失或前序步骤导致跳过 | 作为差异跟踪，不能静默忽略 |
| `WARNING` | 用例完成但发现兼容性警告 | 作为差异跟踪 |
| `XFAIL` | Litmus 自己声明的预期失败 | 当前不写入 AsterDrive known-difference |

默认 Baseline 组的判定是双向严格的：

- 出现 baseline 中没有的 `FAIL`、`SKIPPED` 或 `WARNING`，测试失败；
- baseline 中记录的差异不再出现，测试也失败，提示删除已经过期的豁免；
- 实际用例数与固定版本预期不一致，测试失败；
- 进程状态与解析出的失败状态不一致，测试失败。

`protected` 使用 Probe 模式：仍检查进程状态、超时和固定用例数，但其 `FAIL` / `WARNING` 只写入 `observed_differences`，不进入 AsterDrive 的 RFC known-difference baseline。

所以 baseline 不是“失败都算了”的名单，而是必须持续收敛的兼容性债务清单。

## 更新 known-difference 基线

文件位置：

```text
tests/webdav/fixtures/litmus-baseline.txt
```

每条记录有五列：

```text
group | FAIL|SKIPPED|WARNING | test name | independent tracking issue URL | rationale
```

示意：

```text
locks | FAIL | TEST_NAME | https://github.com/AsterCommunity/AsterDrive/issues/ISSUE | concise reason
```

约束：

- 必须引用独立的 AsterDrive 跟踪 issue；WebDAV 总体路线 issue `#421` 不能代替具体缺陷；
- 同一个分组、状态和用例名不能重复；
- 理由要描述当前可复现的协议差异，不要只写“known issue”；
- 修复后要删除对应记录；stale entry 会让检查失败；
- 先确认是 AsterDrive 行为、反向代理行为还是 Litmus 版本差异，再决定是否入基线。

迁移到 0.18 后暴露的 `props::propextended` 差异已在 [#426](https://github.com/AsterCommunity/AsterDrive/issues/426) 中修复。`PROPFIND` 现在会按照 RFC 4918 Section 17 忽略未知 XML 元素、属性及其完整子树，同时继续按 namespace 识别协议控制元素并执行方法自身的 grammar 校验。XML body 大小、深度、文档格式、DTD 和 entity 安全验证仍先于语义层对扩展子树的忽略。仓库内回归矩阵覆盖顺序变化、namespace 名称冲突、未知子树内嵌已知元素、未知属性、非法 selector 组合，以及被忽略子树内部的安全违规；固定版本 Litmus 0.18 的 `props` 分组现已通过全部 33 个用例，因此对应的 stale baseline 记录已经删除。跨 handler 的 WebDAV XML 扩展语义和 grammar 审计由 [#427](https://github.com/AsterCommunity/AsterDrive/issues/427) 单独跟踪。

修改 baseline 后至少运行：

```bash
cargo test --test webdav litmus_compliance::committed_litmus_baseline_is_well_formed
cargo test --test webdav litmus_compliance::litmus_baseline_requires_independent_tracking_issues
```

然后重新运行受影响的外部分组。

## 直接检查一个已部署的 WebDAV 地址

需要区分“验证当前代码”与“验证部署链路”。仓库内 harness 验证当前 checkout；直接运行 Litmus 可以额外覆盖反向代理、TLS 和部署配置：

```bash
litmus "https://HOST/webdav/" "WEBDAV_USERNAME" "WEBDAV_PASSWORD"
```

只运行一个分组：

```bash
TESTS=locks litmus \
  "https://HOST/webdav/" \
  "WEBDAV_USERNAME" \
  "WEBDAV_PASSWORD"
```

执行前遵守这些边界：

- 使用一次性 WebDAV 账号和独立的空根目录；
- 目标父路径下不能已有名为 `litmus` 的业务目录；
- Litmus 会创建、修改、复制、移动、锁定并删除测试资源；
- 先测试直连 AsterDrive，再测试反向代理后的公开地址，这样才能区分应用行为和代理配置；
- 地址保留结尾 `/`，用户名和密码始终加引号；
- 检查完成后确认测试目录和锁记录已经清理。

## 运行真实客户端兼容性测试

真实客户端测试覆盖 Litmus 之外的实际工作流：

- rclone：列目录、上传、下载、同步、递归复制和移动、特殊文件名、Range 读取；
- curl：WebDAV 方法、Range、COPY/MOVE、LOCK/UNLOCK 和响应头；
- cadaver：交互式客户端的创建目录、上传、下载、移动和清理流程。

CI 使用 `scripts/ci/webdav-compat/install-clients.sh` 安装固定版本。这个安装脚本面向 Linux CI；版本和 SHA-256 同样记录在 `scripts/ci/webdav-compat/versions.env`。

本机已经具备所需客户端后，运行：

```bash
cargo test --test webdav client_e2e:: -- \
  --ignored --nocapture --test-threads=1
```

只跑某个客户端：

```bash
cargo test --test webdav client_e2e::webdav_rclone -- \
  --ignored --nocapture --test-threads=1

cargo test --test webdav client_e2e::webdav_curl -- \
  --ignored --nocapture --test-threads=1

cargo test --test webdav client_e2e::webdav_cadaver -- \
  --ignored --nocapture --test-threads=1
```

本地工具版本和 CI 固定版本不一致时，本地结果用于快速定位，最终兼容性结论以固定版本 CI 为准。

## CI 行为

`.github/workflows/webdav-compatibility.yml` 包含两个 job：

### Litmus baseline

- 在 WebDAV 相关路径的 PR、`master` push、定时任务和手动触发中运行；
- 从固定 Litmus/neon 提交和 SHA-256 构建 Litmus 0.18；
- 串行执行默认五个 ignored Litmus 分组；
- 保存工具版本、测试总日志、分组 `result.json` 和请求日志；
- 产物默认保留 30 天。

### External clients

- 只在定时任务和 `workflow_dispatch` 中运行；
- 安装固定版本的 rclone、curl 和 cadaver；
- 执行 `tests/webdav/client_e2e.rs` 中的 ignored 测试；
- 保存工具版本和完整客户端测试日志。

PR 上 Litmus job 负责快速守住协议基线；真实客户端矩阵更慢，放在定时和手动检查中。

## 修改类型与建议矩阵

| 修改内容 | 最小检查 | 合并前追加检查 |
| --- | --- | --- |
| `Depth`、ETag、`If`、`Destination` | 对应 `test_webdav` 用例 | Litmus 受影响分组 |
| `PROPFIND` / `PROPPATCH` / XML | `protocol` 属性用例 | `litmus_compliance::test_litmus_props`，必要时 rclone/cadaver |
| `MKCOL` / `DELETE` | 对应资源回归 | `litmus_compliance::test_litmus_basic` |
| `COPY` / `MOVE` | 对应资源和条件请求回归 | `litmus_compliance::test_litmus_copymove` + rclone |
| `LOCK` / `UNLOCK` | 锁与 If header 回归 | `litmus_compliance::test_litmus_locks` + curl/cadaver |
| `GET` / `HEAD` / `PUT` / Range | transfer 回归 | `litmus_compliance::test_litmus_http` + rclone/curl |
| Basic Auth、账号 scope、缓存失效 | `protocol` 和 `accounts` 测试 | 全部 Litmus 分组 + 真实客户端 |
| 路径编码、特殊文件名 | 路径回归 | Litmus + rclone/curl/cadaver 对应用例 |
| 反向代理、CORS 或 TLS | 应用层回归 | 对实际部署地址再跑一次客户端检查 |

## 升级 Litmus 基线

将固定基线升级到后续版本时，作为独立变更处理：

1. 阅读目标版本 `NEWS`，确认新增、删除或改名的用例；
2. 在隔离环境直接运行目标 Litmus，先收集原始分组输出；
3. 更新 `scripts/ci/webdav-compat/versions.env`；
4. 更新 `tests/webdav/litmus_compliance.rs` 中的版本、预期用例数和 ignore 说明；
5. 验证输出解析器是否仍能识别 `pass`、`FAIL`、`SKIPPED`、`XFAIL` 和 `WARNING`；
6. 逐条重新判定 known differences，不能把旧 baseline 原样搬过去；
7. 更新 `.github/workflows/webdav-compatibility.yml` 的安装来源和工具版本记录；
8. 先手动运行完整 workflow，再合并版本升级。

`largefile`、`lockbomb` 和 `lockbomb-single` 已作为 ignored 资源测试落在 `tests/webdav/litmus/resource.rs`；`protected` 则作为 ignored 安全策略探针单独放在 `tests/webdav/litmus/security_policy.rs`。它们都不进入普通 PR 门禁。未来把任一可选套件接入 CI 时，要分别设计触发方式、超时、资源消耗、架构适用性和结果基线，尤其不要把大文件或锁压力测试混入普通 PR 快速检查。

## 失败定位速查

| Litmus 分组或现象 | 优先检查 |
| --- | --- |
| `basic` 的 MKCOL/DELETE/PUT | `resources/`、`fs/`、`path_resolver.rs` |
| `copymove` 的 Depth/Overwrite/Destination | `resources/`、`protocol.rs` |
| `props` 的 207/XML/namespace | `props/`、`responses.rs` |
| `locks` 的 token/owner/depth | `locks/`、`db_lock_system.rs`、`protocol.rs` |
| `http` 的 Expect/Range/连接行为 | `transfer/`、`responses.rs` |
| `protected` 的保留路径策略 | `path_resolver.rs`、`fs/mod.rs`、`entity_property.rs`、`property_repo.rs` |
| 直连通过、代理地址失败 | 反向代理方法白名单、请求头透传、body 限制、TLS |
| Litmus 通过、真实客户端失败 | 客户端特有探测顺序、路径编码、重试、同步语义 |
| 本地通过、CI 失败 | 固定工具版本、架构、环境变量和产物日志 |

最后别只看“测试进程是绿的”。合规检查的有效结果应该同时包含：固定工具版本、明确的测试分组、结构化结果、请求轨迹，以及每个 Baseline 保留差异对应的独立跟踪 issue；Probe 组则必须保留原始 `observed_differences` 和适用性判断。
