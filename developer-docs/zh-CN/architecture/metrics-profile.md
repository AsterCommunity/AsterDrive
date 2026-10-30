# Drive Aster Metrics v1

本版采用 OTel Semantic Conventions 1.44.0，Resource 固定 aster.metrics.schema.version=aster-metrics/v1。Prometheus 仍为显式启用 pull，OTLP HTTP/protobuf 可独立启用；不安装全局 provider，不增加生产接线。

完整逐 instrument 合同由真实 descriptor 生成到 docs/public/metrics/asterdrive-v1.json，包含 OTel 名、Prometheus 名、类型、单位、属性和桶，测试逐项核对。生成命令：cargo run -p aster_drive_metrics --all-features --example export_catalog。

## 角色与适用性

| 角色 | 已实现观测 | 事实源和范围 |
| --- | --- | --- |
| primary | HTTP server、DB operation、原生 pool、process、ready/health、上传/下载/Blob、远端协议、任务 handler/状态转换 | 进程局部事件；数据库队列/节点快照是共享库投影，跨副本不相加 |
| follower | HTTP server、DB operation、原生 pool、process、ready/health、远端协议/数据面、staging 容量 | 无 primary 上传/Blob/任务队列，不为不适用项补零 |
| CLI/migration | 原有日志和退出结果 | 不新增常驻 metrics listener |

ready 取 /health/ready 同一结果，首次检查前未知；diagnostic health 0/1/2 不等同 readiness。HTTP duration/active 覆盖响应体 EOF/error/drop，固定 probe/scrape 路径排除，未知业务 404 继续记录；未知 route 省略。scheme 取 transport TLS/URI，反代 HTTPS 的可信规范化属于部署入口。

CPU 为秒累计快照，不重复 Add；内存是物理进程内存，不等同容器总量。pool 只输出 SQLx 实际 idle/used/max，单 writer 不造 reader。任务 handler 的 returned_ok/returned_error/dropped 是本地事实，持久成功另由已确认状态转换表达；取消 active 在实际 future drop/返回时下降。

Drive 的主数据库池身份固定为 main，实际池名为 main.writer / main.reader；不从数据库 URL、用户输入或运行时 ID 生成。HTTP status 属性在 OTLP 中为整数，Prometheus 标签为文本；生成目录的 integer_attributes 标注整数属性。HEAD 在响应可用时结束计量，不把协议省略 body 记为取消。

## 产品指标和基数

49 个产品 instrument 使用 aster.drive.*，保留原有业务成功、失败、取消及字节边界。属性来自 enum/static catalog，route 为匹配模板。profile/rule/policy 仅既有受控拓扑目录，不含文件、用户、请求、URL、对象 key、SQL、错误文本。默认每 instrument 4096、实例 16384 个组合；Classic histogram 还展开桶/sum/count，部署预算须乘副本和环境。超限跳过本次观测，累计本地失败，不改变业务事务。

共享 upload/Blob/remote/task 快照有效期为 180 秒，后台采样失败保留 last-good 和旧时间。首次成功前省略，真实空集合为零；查询同时过滤 valid=1 和 freshness。同一共享库取新鲜成功代表，不 sum primary 副本；独立库通过不同 job/target 分开。无数据不补零。capacity 是独立最小值，不能相加或用不同目标的 min 推导比例。

## 迁移与限制

应用、生成目录和 Grafana 一起升级；查询加 schema 过滤，删除旧序列，不长期双发、不改写历史。自定义 rules/alerts 按目录更新；回退使用配套旧镜像/旧看板。pull/OTLP 双路进入同一存储只能消费一路。

当前是 Classic Histogram，未宣称 Native。SQLx 未公开累计等待/获取连接超时，省略而不伪造。通用 outbound HTTP/RPC、完整任务 lease/recovery/finish、事务 write-result 尚无全链 typed 证据；不把逻辑 driver 次数冒充 HTTP attempt，不宣称完整实现 AutoBits RFC 0007 全部角色组。后续补真实 adapter 和取消/失败测试，保持唯一计量 owner。

本地 patch、固定 Git 依赖、库测试、Drive 测试、发行镜像、Collector 接收及生产 rollout 是不同证据，目录不证明线上完成。
