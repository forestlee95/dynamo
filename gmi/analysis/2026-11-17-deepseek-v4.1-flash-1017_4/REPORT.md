# deepseek-ai/deepseek-v4.1-flash-1017_4：过去24小时失败事件分析

- **时间窗（UTC）**：2026-11-16T03:37:54Z → 2026-11-17T03:37:54Z
- **对象**：`backend.endpoint.id = deepseek-ai/deepseek-v4.1-flash-1017_4`（vendor `gmi__gpu__Sen-DSV41-1017-v1`，后端 `rke2-aps2-svc-58f72dd1ead99337.elb.ap-south-2.amazonaws.com:32027` → `dynamo-*-dsv41-prod-1`，Dynamo 1.6.1 + SGLang v0.5.23，prefill/decode 分离）
- **数据源**：控制台 `https://ie-log.gmi-serving.com`；Trace 来自服务 `ieops-v2-gateway-prod`（含 forward-worker / routing / dynamo 子 span），日志来自 OQL `/api/v1/query/oql`
- **口径**：一次“失败事件” = 一个落在 `_4` 上、`http.status_code != 200` 的 `llm_request_handler.execute_request` span（即对 `_4` 的一次失败尝试）。24h 内 **3,644 个失败事件，分布在 3,644 条 trace（一条 trace 一个）**，占 `_4` 总请求 206,634 的 **1.76%**。
- **完整性校验**：每小时 `(总数 − 200 数)` 与 `status.code=Error` 的数量逐小时一致（合计 2,926）。本报告的集合额外包含了 `_4` 失败后重试到 `_1` 成功的 trace，所以会更大一些。
- **修复计划**：已单独整理到同目录的 `FIX_PLAN.md`。
- **说明**：本文是 GMI fork 内部的分析记录。文中 `out/…` 开头的路径指分析工作区 `obs-console-playground` 中的数据与脚本，未随本文提交。

> **修订记录（2026-11-17，第二版）**
> 1. E 类（Dynamo frontend 请求校验）和 F1–F4（SGLang worker 多模态 / 结构化输出）**归因 Dynamo**，不再算作客户端问题。
> 2. **G1/G2 改判**：原“Dynamo KV 传输失败（379）/ 流未关闭（116）”中，除 F5 外的 490 个，经事件顺序核对（第 2 节），实际是 **Dynamo 收到请求后，与 forward-worker 之间的连接中断**。Dynamo 因此取消 decode，prefill 的 `KVTransferError` 只是取消的副作用。根因在链路，不在 Dynamo。
> 3. **A3 改判**：约 80% 是 Dynamo 先看到连接断开，而不是调用方没耐心。
> 4. **F5 改判**：不是“KV bootstrap 超时被误报为 400”，而是 decode 以 InvalidArgument 拒绝了多模态请求；prefill 因此空等 300s 才超时。

> 注意：dashboard 上的 “HTTP Request Error Count” 面板会过滤掉 400/429，而且 499 的 status 记为 `0`，所以面板上看起来只有约 1.2k 个错误，明显低估了。

---

## 1. 总览

| 大类 | 子类 | 数量 | 占比 | 调用方最终结果 | 责任方 |
|---|---|---:|---:|---|---|
| A 客户端断开 499 | **A1** 客户端断开(499)·请求未到达Dynamo(连接挂起) | 1453 | 39.9% | 499:1453 | 链路（请求未到达 Dynamo） |
| A 客户端断开 499 | **A2** 客户端断开(499)·Dynamo已收到后连接中断(KV报错为副作用) | 225 | 6.2% | 499:225 | 链路（Dynamo 已收到后连接中断；约 22% 为调用方先放弃） |
| A 客户端断开 499 | **A3** 客户端断开(499)·已到Dynamo无错误(约80%为连接先断) | 160 | 4.4% | 499:160 | 大部分为链路（约 80%），其余为调用方 |
| B 504 上游超时 | **B1** 504·120s上游超时·请求未到达Dynamo(连接挂起) | 802 | 22.0% | 200:481 / 400:17 / 499:304 | 链路（请求未到达 Dynamo） |
| B 504 上游超时 | **B2** 504·120s·Dynamo已收到后连接中断(KV报错为副作用) | 47 | 1.3% | 200:15 / 499:32 | 链路（Dynamo 已收到后连接中断） |
| B 504 上游超时 | **B3** 504·120s·Dynamo已收到后连接中断 | 54 | 1.5% | 200:16 / 499:38 | 链路（Dynamo 已收到后连接中断） |
| B 504 上游超时 | **B4** 504·拨号/DNS超时(5s)连NLB失败 | 4 | 0.1% | 200:4 | 链路（拨号 / DNS） |
| C 504 流无内容 | **C1** 504·流120s无内容·Dynamo已收到后连接中断(KV报错为副作用) | 93 | 2.6% | 200:32 / 499:61 | 链路（Dynamo 已收到后连接中断） |
| C 504 流无内容 | **C2** 504·流120s无内容·Dynamo已收到后连接中断 | 49 | 1.3% | 200:27 / 499:22 | 链路（Dynamo 已收到后连接中断） |
| D 502 上游不可用 | **D1** 502·connection reset by peer(复用连接被RST) | 124 | 3.4% | 200:122 / 400:1 / 499:1 | 链路（复用连接被 RST） |
| D 502 上游不可用 | **D2** 502·Dynamo已收到后连接中断,网关快速感知(KV报错为副作用) | 9 | 0.2% | 200:9 | 链路（Dynamo 已收到后连接中断） |
| D 502 上游不可用 | **D3** 502·Dynamo已收到后连接中断,网关快速感知 | 13 | 0.4% | 200:12 / 400:1 | 链路（Dynamo 已收到后连接中断） |
| E 400 Dynamo frontend 校验 | **E1** 400·上下文超长(>1048576 tokens) | 134 | 3.7% | 400:134 | Dynamo frontend（Rust HTTP 校验） |
| E 400 Dynamo frontend 校验 | **E2** 400·tools schema/函数名校验失败 | 57 | 1.6% | 400:57 | Dynamo frontend（Rust HTTP 校验） |
| E 400 Dynamo frontend 校验 | **E3** 400·请求体反序列化失败(不支持的字段/枚举值) | 49 | 1.3% | 400:49 | Dynamo frontend（Rust HTTP 校验） |
| E 400 Dynamo frontend 校验 | **E4** 400·历史tool_calls.arguments非法JSON | 22 | 0.6% | 400:22 | Dynamo frontend（Rust HTTP 校验） |
| E 400 Dynamo frontend 校验 | **E5** 400·Invalid request(无细节) | 46 | 1.3% | 400:46 | Dynamo frontend（Rust HTTP 校验） |
| E 400 Dynamo frontend 校验 | **E6** 400·其他参数校验错误 | 1 | 0.0% | 400:1 | Dynamo frontend（Rust HTTP 校验） |
| F 400 SGLang worker 流内报错 | **F1** 400帧·xgrammar不支持的JSON Schema(response_format) | 144 | 4.0% | 400:144 | Dynamo（SGLang worker · xgrammar） |
| F 400 SGLang worker 流内报错 | **F2** 400帧·非图片媒体(音频/PDF/HEIC)作为image_url传入 | 125 | 3.4% | 400:125 | Dynamo（多模态支持） |
| F 400 SGLang worker 流内报错 | **F3** 400帧·图片解码失败(png/jpeg/gif) | 25 | 0.7% | 400:25 | Dynamo（多模态支持） |
| F 400 SGLang worker 流内报错 | **F4** 400帧·prefill时上下文超长 | 3 | 0.1% | 400:3 | Dynamo（frontend 对含图请求跳过长度预检） |
| F 400 SGLang worker 流内报错 | **F5** 400帧·decode拒绝多模态请求(InvalidArgument),prefill空等300s | 5 | 0.1% | 400:5 | Dynamo（decode 拒绝多模态 + prefill 未取消） |

**按责任方汇总**

| 责任方 | 子类 | 次数 | 占比 |
|---|---|---:|---:|
| 链路（forward-worker ↔ AWS NLB ↔ Dynamo frontend） | A1 B1 B4 D1（请求未到达 Dynamo）+ A2 B2 B3 C1 C2 D2 D3（Dynamo 收到后连接中断） | 2,873 | 78.8% |
| Dynamo（frontend 请求校验、SGLang worker 多模态 / 结构化输出） | E1–E6、F1–F5 | 611 | 16.8% |
| 已到 Dynamo 且无错误，首 token 前断开 | A3（其中约 80% 实为链路中断） | 160 | 4.4% |

A2 中约 22% 两端感知时间相差在 1s 以内，可能是调用方真正先放弃。按“谁先断”精确计算，链路占比约 80%。

**结论要点**

1. **约 80% 的失败是 forward-worker 与 Dynamo frontend 之间的连接中断，而且 forward-worker 侧感知不到。** 有时请求根本没到 Dynamo（A1/B1），有时 Dynamo 收到后连接被断开（A2/B2/B3/C1/C2）。中断后网关等到调用方放弃（≈10s）或 120s 超时；只有少数情况能快速收到 RST / EOF 并失败（D1/D2/D3）。
2. **Dynamo 自身问题共 611 个（16.8%），全部是 400 类**：frontend 请求校验过严或兼容性不足（309），以及 SGLang worker 的结构化输出和多模态处理（302）。
3. **Dynamo 有多处可观测性缺口，影响定位**：
   - 断开日志 `Stream closed unexpectedly` 不带 trace_id、对端地址和断开原因。
   - 路由失败只记 `error.type`，不记具体错误信息。
   - decode 被取消后，prefill 打出的 KVTransferError 会被误读为 KV 故障。
4. 网关把 Dynamo 返回的 400 统一标成 `unsupported_operation`，掩盖了真实原因。这是网关侧的问题。

---

## 2. 关键证据：Dynamo 收到请求后的连接中断（修订 G1/G2/A3 归因）

### 2.1 事件顺序：先断开，后 KV 报错

在同时有 decode 路由 span 和 prefill `KVTransferError` 的 372 条 trace 中，**全部 372 条（100%）都是 frontend 先结束 decode 路由（`cancelled / stream_drop`），之后 prefill 才报错**。KV 报错比 decode 被取消晚：p10 26ms，p50 121ms，p90 1,950ms。

其中 1 条 trace 在同一次网关尝试里出现了两个 Dynamo 请求，两对都是先取消、后报错。计算方法见 `out/scripts/timeline_evidence.py`。

decode 被 `kill` 后，它的 KV 接收端随之销毁，prefill 发送 KV 时就会报 `Failed due to an unknown reason from another rank`；bootstrap 阶段则表现为 `Prefill bootstrap failed`。

### 2.2 两端谁先感知断开

下表的“网关比 Dynamo 晚”= 网关这次尝试结束的时间 − Dynamo frontend 记录 `cancelled` 的时间。

| 子类 | 样本 | 响应头发出 → Dynamo 见断开（p50） | 网关比 Dynamo 晚（p10 / p50 / p90） | 1s 内一致的比例 |
|---|---:|---:|---|---:|
| B2 | 47 | 200ms | 116.4s / 118.6s / 119.6s | 0% |
| B3 | 49 | 213ms | 116.6s / 118.2s / 119.5s | 0% |
| C1 | 93 | 830ms | 113.9s / 118.2s / 119.5s | 0% |
| C2 | 43 | 620ms | 115.6s / 118.7s / 119.3s | 0% |
| A2 | 223 | 313ms | −0.1s / 11.0s / 71.0s | 22% |
| A3 | 125 | — | −0.1s / 12.4s / 88.4s | 20% |
| D2 | 9 | 153ms | 0.08s / 0.10s / 0.26s | 100% |
| D3 | 10 | 477ms | 0.12s / 0.15s / 24.6s | 90% |

D 类两端相差约 100ms，说明两边的时钟对得上，B/C 类那 118s 的差距不是时钟误差造成的。B/C 类里，Dynamo 在约 0.2–0.8s 就认定客户端断开了，而网关侧什么都没收到。

### 2.3 代码机制（Dynamo 1.6.1）

- `stream_drop` 是 `lib/runtime/src/pipeline/network/egress/route_span.rs` 中 `RouteSpanStream` 的 `Drop` 实现记录的：消费方在流结束前把它丢弃了。源码注释写明，这种情况发生在“HTTP 响应体被丢弃之后”。
- 响应体被 axum/hyper 丢弃后，`lib/llm/src/http/service/disconnect.rs` 的 `connection_monitor` 打出 `WARN Stream closed unexpectedly; issuing cancellation`，然后执行 `engine_context.kill()`，decode 随之收到 `kill`。
- 生产日志可以对上：C1 例子在 `04:36:04.143641`（与 `stream_drop` 同一毫秒）有一条 `Stream closed unexpectedly` 日志。但这条日志**没有 trace_id**：monitor 是 `tokio::spawn` 出去的任务，没有挂到请求的 span 上，所以按 trace 查日志时查不到它。
- frontend 使用默认的 `axum::serve`：不记录对端 ip:port，也不开 TCP keepalive。

### 2.4 典型时间线（C1，trace `2a6fc286ab0fa11175ca8762b2d3a952`；时间相对 `_4` 那次 execute_request 的开始）

| 时间 | 事件 |
|---:|---|
| 0ms | gateway 开始调用 `_4` |
| 80ms | Dynamo frontend 收到请求 |
| 451 / 461ms | frontend 分别路由 prefill / decode |
| 473ms | frontend 发出 `200` 响应头 |
| **1,700ms** | **frontend：`Stream closed unexpectedly`，decode 路由 `cancelled / stream_drop`，记 `cancelled before completion`** |
| 1,701ms | decode：`request cancellation received signal=kill source=upstream` |
| 1,710ms | decode：`Failed to publish complete final for stream`（客户端已不在） |
| 3,294ms | prefill：`Prefill transfer failed ... KVTransferError ... unknown reason from another rank`（副作用） |
| 120,019ms | gateway：`Backend stream timed out before returning any content` → 504，重试 `_1` |
| 124,622ms | `_1` 返回 200，调用方 TTFT = 124.7s |

### 2.5 链路根因：待验证的假设

请求没到达 Dynamo（A1/B1）、进行中的连接被断开（B2/B3/C*）、复用连接被 RST（D1），这三种现象很可能是同一个链路问题。最具体的待验证假设如下：

**AWS NLB 的 client IP preservation 与源端 NAT 端口复用冲突。**

- AWS 文档明确说明了这一限制：开启 client IP preservation 时，如果客户端（或客户端前面的 NAT）用**同一个源 ip:port 同时连接多个 NLB 节点**，而这些连接被路由到同一个 target，target 会把它们看成同一个 socket，导致连接错误。
- 本链路的条件都满足：
  - forward-worker 在 GCP，出口经过 NAT。
  - NLB 有两个节点 IP，`18.61.206.186` 和 `16.112.141.252`，两个都出现在了 RST 报错里。
  - target 是 NodePort `:32027`（instance target 默认开启 client IP preservation）。
- 这个假设能同时解释三种现象：新连接把已有连接“撞掉”（Dynamo 侧看到断开，forward-worker 侧收不到）、请求被黑洞（A1/B1）、以及 RST（D1）。

**验证方法（不涉及 Dynamo 代码）**

1. 查 NLB target group 的 `preserve_client_ip.enabled`、是否开启 cross-zone，以及 GCP Cloud NAT 的端口分配和 endpoint-independent mapping 配置。
2. A/B 试验：让 forward-worker 固定只连其中一个 NLB IP，或关闭 client IP preservation，观察 A1/B1/C1 是否显著下降。
3. 在 Dynamo 节点上看 `conntrack -S` 中的 `insert_failed` / `drop`，以及 `nstat -az` 中的 `TcpExtTCPChallengeACK` / `TcpExtTCPAbortOnData` 是否和失败时间同步上升。
4. 结合 `FIX_PLAN.md` 中 Dynamo 侧补充的对端 ip:port 日志，检查同一时刻是否有两个连接来自相同的源 ip:port。

---

## 3. 分类详解

### A. 客户端断开（499；`execute_request` 的 status=0，`error.message="forward service: context canceled"`，`gmi.routing.stop_cause=client_disconnected`）

共 1838 个。调用方在**拿到首个 token 之前**断开（`ttft_ms=0`、`body_delivered=false`）。499 只是表象，按“请求有没有到达 Dynamo”和“Dynamo 与网关谁先感知断开”拆成三类：

#### A1 请求未到达 Dynamo，连接挂起后调用方放弃（1453 个）

**证据**
- trace 中只有 gateway → forward-worker（`forward_worker.vendor_send` 一直挂到调用方断开），**没有任何 `dynamo-*` span**。而成功请求的基线（抽样 658 条 200）**100%** 都有 Dynamo span，所以可以判断请求没有被 Dynamo frontend 处理。
- forward-worker 日志：`sender: vendor request failed  error=context canceled  failure_kind=caller_canceled  status_code=499`
- 连接特征（vendor_send span）与成功基线对比：

  | 指标 | A1/B1 失败 | 200 基线 |
  |---|---|---|
  | `gmi.conn.reused=true` | ≈100% | 86% |
  | `gmi.conn.idle_ms` p10 / p50 / p90 | 16s / 37s / 75s | 0 / 8s / 46s |
  | idle > 60s 的比例 | 24% | 6% |
  | `gmi.tcp.done.*`（请求结束时的 TCP 统计） | 缺失 | 有 |

- 不集中在某个 forward-worker pod；每小时稳定在约 1%，低峰期更高。
- 调用方放弃耗时：p10 10.2s，p50 16s，p90 75s。

**根因判断（修订）**：forward-worker → AWS NLB（`:32027`，两个 NLB IP：`18.61.206.186`、`16.112.141.252`）→ Dynamo frontend 这条跨云链路上的连接失效，forward-worker 侧却收不到任何信号。第 2 节的证据表明，**正在进行中的连接也会断**（Dynamo 在发出响应头后 150ms–4.5s 内看到断开），所以这不只是“空闲连接被静默回收”的问题。最具体的待验证假设见 2.5 节。


**例子（A1）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `ffa17e83a0feb472026e5bf7122a591d` | 2026-11-16T03:38:37 | `_4:0` → **499** | 44490ms | conn reused=true idle=20556ms<br>**无 Dynamo span**<br>body=148851B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/ffa17e83a0feb472026e5bf7122a591d) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A37%3A37.248Z&to=2026-11-16T03%3A41%3A21.738Z&oql=body%3A+%22ffa17e83a0feb472026e5bf7122a591d%22+and+NOT+body%3A+%22request_data%22) |
| `45dda6409df315805ac54b6d71cdebd3` | 2026-11-16T08:41:12 | `_4:0` → **499** | 54161ms | conn reused=true idle=48681ms<br>**无 Dynamo span**<br>body=216334B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/45dda6409df315805ac54b6d71cdebd3) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A40%3A12.749Z&to=2026-11-16T08%3A44%3A06.910Z&oql=body%3A+%2245dda6409df315805ac54b6d71cdebd3%22+and+NOT+body%3A+%22request_data%22) |
| `dce29ab436e6770ea15947eb28aa91c8` | 2026-11-16T14:35:45 | `_4:0` → **499** | 113539ms | conn reused=true idle=16221ms<br>**无 Dynamo span**<br>body=581052B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/dce29ab436e6770ea15947eb28aa91c8) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A34%3A45.331Z&to=2026-11-16T14%3A39%3A38.870Z&oql=body%3A+%22dce29ab436e6770ea15947eb28aa91c8%22+and+NOT+body%3A+%22request_data%22) |
| `a05dd193c285bbc718cb8653646ff65d` | 2026-11-16T21:04:16 | `_4:0` → **499** | 64197ms | conn reused=true idle=19054ms<br>**无 Dynamo span**<br>body=301697B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/a05dd193c285bbc718cb8653646ff65d) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T21%3A03%3A16.495Z&to=2026-11-16T21%3A07%3A20.692Z&oql=body%3A+%22a05dd193c285bbc718cb8653646ff65d%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/A1.txt`

#### A2 Dynamo 已收到请求后连接中断，调用方随后放弃（225 个；prefill 的 KVTransferError 是副作用）

**证据**
- 220 个的 decode 路由以 `cancelled / stream_drop` 结束：Dynamo frontend 的 HTTP 响应体被丢弃，frontend 记 `cancelled before completion`，并打出 `WARN Stream closed unexpectedly; issuing cancellation`。随后 decode 收到 `kill`，prefill 才报 `KVTransferError ... Failed due to an unknown reason from another rank`。
- 响应头发出到 Dynamo 看到断开：p10 / p50 / p90 = 149 / 313 / 3,763ms。
- 两端感知对比（有 frontend 取消记录的 223 个）：**约 78%** Dynamo 先看到断开，比网关的 499 早 p50 约 11s；**约 22%** 两者相差在 1s 以内，属于真正的调用方先放弃，KV 报错同样只是取消的副作用。

**结论**：不是 Dynamo 的问题，根因同 A1（链路）。Dynamo 侧只需要改可观测性，见 `FIX_PLAN.md`。


**例子（A2）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `242437699722f311ba2a9389b975fb0a` | 2026-11-16T03:43:39 | `_4:0` → **499** | 59853ms | conn reused=true idle=12237ms<br>时间线：响应头 67ms → Dynamo 见断开 222ms → prefill KV 报错 252ms；网关比 Dynamo 晚 59630ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=2846B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/242437699722f311ba2a9389b975fb0a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A42%3A39.014Z&to=2026-11-16T03%3A46%3A38.867Z&oql=body%3A+%22242437699722f311ba2a9389b975fb0a%22+and+NOT+body%3A+%22request_data%22) |
| `20aabe7aa885f447499d9f63e434f207` | 2026-11-16T09:15:06 | `_4:0` → **499** | 10759ms | conn reused=false idle=0ms<br>时间线：响应头 340ms → Dynamo 见断开 10859ms → prefill KV 报错 10893ms；网关比 Dynamo 晚 -101ms 结束<br>prefill（副作用）: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an`<br>body=3826B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/20aabe7aa885f447499d9f63e434f207) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A14%3A06.106Z&to=2026-11-16T09%3A17%3A16.865Z&oql=body%3A+%2220aabe7aa885f447499d9f63e434f207%22+and+NOT+body%3A+%22request_data%22) |
| `5eeee9dca4c0ba40e96087a806ace0ad` | 2026-11-16T17:35:47 | `_4:0` → **499** | 11346ms | conn reused=false idle=0ms<br>时间线：响应头 206ms → Dynamo 见断开 519ms → prefill KV 报错 551ms；网关比 Dynamo 晚 10828ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=6167B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/5eeee9dca4c0ba40e96087a806ace0ad) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T17%3A34%3A47.642Z&to=2026-11-16T17%3A37%3A58.988Z&oql=body%3A+%225eeee9dca4c0ba40e96087a806ace0ad%22+and+NOT+body%3A+%22request_data%22) |
| `b2352bab08b54bf646ec8bd50ff3d8bf` | 2026-11-16T20:00:26 | `_4:0` → **499** | 17676ms | conn reused=true idle=20353ms<br>时间线：响应头 124ms → Dynamo 见断开 456ms → prefill KV 报错 655ms；网关比 Dynamo 晚 17220ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=32946B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/b2352bab08b54bf646ec8bd50ff3d8bf) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T19%3A59%3A26.945Z&to=2026-11-16T20%3A02%3A44.621Z&oql=body%3A+%22b2352bab08b54bf646ec8bd50ff3d8bf%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/A2.txt`

#### A3 已到 Dynamo，Dynamo 无错误，首 token 前断开（160 个）

**证据**：其中 125 个有 frontend 的取消记录（`cancelled before completion`）。**约 80%** 是 Dynamo 先看到连接断开，比网关的 499 早 p50 约 12.4s；只有约 20% 两者在 1s 以内，属于真正的调用方先放弃。

**结论（修订）**：原先判为“调用方耐心”，实际大部分也是链路中断，归入链路问题。


**例子（A3）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `23768fd30982a56d5d688567ec625033` | 2026-11-16T03:40:23 | `_4:0` → **499** | 120019ms | conn reused=false idle=0ms<br>时间线：响应头 未发出ms → Dynamo 见断开 1249ms；网关比 Dynamo 晚 118769ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=37845B org=a290e350… | [trace](https://ie-log.gmi-serving.com/trace/23768fd30982a56d5d688567ec625033) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A39%3A23.194Z&to=2026-11-16T03%3A44%3A23.213Z&oql=body%3A+%2223768fd30982a56d5d688567ec625033%22+and+NOT+body%3A+%22request_data%22) |
| `ff93b31484bfcf3eb3cd616c1adbafc5` | 2026-11-16T09:23:52 | `_4:0` → **499** | 90718ms | conn reused=true idle=5314ms<br>时间线：响应头 308ms → Dynamo 见断开 2521ms；网关比 Dynamo 晚 88197ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=331649B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/ff93b31484bfcf3eb3cd616c1adbafc5) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A22%3A52.817Z&to=2026-11-16T09%3A27%3A23.535Z&oql=body%3A+%22ff93b31484bfcf3eb3cd616c1adbafc5%22+and+NOT+body%3A+%22request_data%22) |
| `cac168c97e226c867b5913484201eae8` | 2026-11-16T16:04:51 | `_4:0` → **499** | 44990ms | conn reused=true idle=141ms<br>时间线：响应头 431ms → Dynamo 见断开 1185ms；网关比 Dynamo 晚 43805ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=122229B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/cac168c97e226c867b5913484201eae8) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T16%3A03%3A51.027Z&to=2026-11-16T16%3A07%3A36.017Z&oql=body%3A+%22cac168c97e226c867b5913484201eae8%22+and+NOT+body%3A+%22request_data%22) |
| `04813e400c7b8a585a8fc6305e4f40c6` | 2026-11-16T21:31:49 | `_4:0` → **499** | 119926ms | conn reused=true idle=0ms<br>body=791893B org=425da563… | [trace](https://ie-log.gmi-serving.com/trace/04813e400c7b8a585a8fc6305e4f40c6) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T21%3A30%3A49.700Z&to=2026-11-16T21%3A35%3A49.626Z&oql=body%3A+%2204813e400c7b8a585a8fc6305e4f40c6%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/A3.txt`

### B. 504 上游超时（`error.type="timeout: Upstream service timed out"`；forward-worker `context deadline exceeded`，固定 **120s**）

共 907 个。这一类在 `_4` 上挂满 120s 后由网关重试到 `_1`（APIHub）：57% 最终 200（但端到端已经超过 120s），41% 调用方先放弃（499），2% 重试到 `_1` 后又被拒绝。B1 是请求没到 Dynamo；B2/B3 是请求到了 Dynamo，但连接随即中断，Dynamo 在约 0.2s 就看到断开，网关却等满 120s。

#### B1 请求未到达 Dynamo，连接挂起 120s（802 个）

**证据**：与 A1 完全相同，没有任何 Dynamo span，连接复用，idle p50 = 37s。区别只在于客户端耐心更长（或是不设超时的客户端），所以撑到了 forward-worker 的 120s deadline。forward-worker 日志：`error=context deadline exceeded  error_type=timeout  failure_kind=transport  status_code=504`。B1 请求体更大（p50 ≈ 458KB，基线 81KB），说明大请求体在半开连接上更容易卡住，或者更容易遇到写阻塞。

**修复**：同 A1（连接池 + header 超时 + 在新连接上重试）。


**例子（B1）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `1ad76d8bc0acb24bc175d887f879be52` | 2026-11-16T03:43:03 | `_4:504>_1:200` → **200** | 120011ms | fw: `context deadline exceeded`<br>conn reused=true idle=20581ms<br>**无 Dynamo span**<br>body=147856B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/1ad76d8bc0acb24bc175d887f879be52) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A42%3A03.213Z&to=2026-11-16T03%3A47%3A03.224Z&oql=body%3A+%221ad76d8bc0acb24bc175d887f879be52%22+and+NOT+body%3A+%22request_data%22) |
| `8e2e91cb52e5689a5e2f3d3bcb6f18d0` | 2026-11-16T09:36:28 | `_4:504>_1:0` → **499** | 120050ms | fw: `context deadline exceeded`<br>conn reused=true idle=20953ms<br>**无 Dynamo span**<br>body=3409327B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/8e2e91cb52e5689a5e2f3d3bcb6f18d0) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A35%3A28.015Z&to=2026-11-16T09%3A40%3A28.065Z&oql=body%3A+%228e2e91cb52e5689a5e2f3d3bcb6f18d0%22+and+NOT+body%3A+%22request_data%22) |
| `04d05b64388f94324b7f5564944e7895` | 2026-11-16T14:55:40 | `_4:504>_1:0` → **499** | 120033ms | fw: `context deadline exceeded`<br>conn reused=true idle=18227ms<br>**无 Dynamo span**<br>body=2441974B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/04d05b64388f94324b7f5564944e7895) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A54%3A40.739Z&to=2026-11-16T14%3A59%3A40.772Z&oql=body%3A+%2204d05b64388f94324b7f5564944e7895%22+and+NOT+body%3A+%22request_data%22) |
| `0f9f79c730fd7274c7fb18e5c2180b4a` | 2026-11-16T20:35:54 | `_4:504>_1:0` → **499** | 120045ms | fw: `context deadline exceeded`<br>conn reused=true idle=27708ms<br>**无 Dynamo span**<br>body=2667889B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/0f9f79c730fd7274c7fb18e5c2180b4a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T20%3A34%3A54.460Z&to=2026-11-16T20%3A39%3A54.505Z&oql=body%3A+%220f9f79c730fd7274c7fb18e5c2180b4a%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/B1.txt`

#### B2 Dynamo 已收到请求后连接中断，网关等满 120s（47 个；KVTransferError 是副作用）

**证据**：decode 路由 `cancelled / stream_drop` 44 个、`kill` 3 个。响应头发出后 p50 200ms，Dynamo 就看到断开；网关结束时间比 Dynamo 晚 p50 **118.6s**，没有一个在 1s 以内。

**结论**：链路问题，Dynamo 本身没有出错。


**例子（B2）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `0622f595630a94dcc743ee7189ce55e4` | 2026-11-16T05:38:56 | `_4:504>_1:0` → **499** | 120029ms | fw: `context deadline exceeded`<br>conn reused=true idle=756ms<br>时间线：响应头 1770ms → Dynamo 见断开 1988ms → prefill KV 报错 302148ms；网关比 Dynamo 晚 118042ms 结束<br>prefill（副作用）: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Request <n> time`<br>body=1982204B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/0622f595630a94dcc743ee7189ce55e4) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T05%3A37%3A56.268Z&to=2026-11-16T05%3A42%3A56.297Z&oql=body%3A+%220622f595630a94dcc743ee7189ce55e4%22+and+NOT+body%3A+%22request_data%22) |
| `d9dcd0b24a85b80077d5cabd65d76bf4` | 2026-11-16T09:55:56 | `_4:504>_1:0` → **499** | 120040ms | fw: `context deadline exceeded`<br>conn reused=false idle=0ms<br>时间线：响应头 6307ms → Dynamo 见断开 6520ms → prefill KV 报错 6601ms；网关比 Dynamo 晚 113520ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=2977484B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/d9dcd0b24a85b80077d5cabd65d76bf4) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A54%3A56.850Z&to=2026-11-16T09%3A59%3A56.890Z&oql=body%3A+%22d9dcd0b24a85b80077d5cabd65d76bf4%22+and+NOT+body%3A+%22request_data%22) |
| `4a2eadab007b01452d5869b699a47093` | 2026-11-16T14:08:54 | `_4:504>_1:0` → **499** | 120062ms | fw: `context deadline exceeded`<br>conn reused=true idle=70271ms<br>时间线：响应头 839ms → Dynamo 见断开 882ms → prefill KV 报错 1314ms；网关比 Dynamo 晚 119180ms 结束<br>prefill（副作用）: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an`<br>body=4143065B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/4a2eadab007b01452d5869b699a47093) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A07%3A54.198Z&to=2026-11-16T14%3A12%3A54.260Z&oql=body%3A+%224a2eadab007b01452d5869b699a47093%22+and+NOT+body%3A+%22request_data%22) |
| `769e7caf105ae1b51bde75370b9ebaa8` | 2026-11-16T21:10:49 | `_4:504>_1:0` → **499** | 120021ms | fw: `context deadline exceeded`<br>conn reused=false idle=0ms<br>时间线：响应头 1963ms → Dynamo 见断开 2096ms → prefill KV 报错 2106ms；网关比 Dynamo 晚 117925ms 结束<br>prefill（副作用）: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an`<br>body=1348105B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/769e7caf105ae1b51bde75370b9ebaa8) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T21%3A09%3A49.659Z&to=2026-11-16T21%3A14%3A49.680Z&oql=body%3A+%22769e7caf105ae1b51bde75370b9ebaa8%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/B2.txt`

#### B3 Dynamo 已收到请求后连接中断，网关等满 120s（54 个）

**证据**
- 18 个在响应头发出后约 213ms 看到 `stream_drop`。
- 35 个在 frontend 预处理阶段（尚未发出响应头）就被判为客户端断开：31 个 `cancelled before completion`；另有 2 个 frontend 已经返回了 400 validation，但这个响应**没有到达网关**。
- 1 个 decode 正常完成。
- 网关结束时间比 Dynamo 晚 p50 118.2s。

**结论**：链路问题。


**例子（B3）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `114252c11ecd8418873ad03b56c4e3c5` | 2026-11-16T03:39:01 | `_4:504>_1:200` → **200** | 120010ms | fw: `context deadline exceeded`<br>conn reused=true idle=46773ms<br>时间线：响应头 216ms → Dynamo 见断开 430ms；网关比 Dynamo 晚 119580ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=438503B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/114252c11ecd8418873ad03b56c4e3c5) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A38%3A01.897Z&to=2026-11-16T03%3A43%3A01.907Z&oql=body%3A+%22114252c11ecd8418873ad03b56c4e3c5%22+and+NOT+body%3A+%22request_data%22) |
| `dbde9e38ecad8ff2efe0cc8ac836093e` | 2026-11-16T09:39:48 | `_4:504>_1:0` → **499** | 120067ms | fw: `context deadline exceeded`<br>conn reused=true idle=4298ms<br>时间线：响应头 未发出ms → Dynamo 见断开 2103ms；网关比 Dynamo 晚 117963ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=4183914B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/dbde9e38ecad8ff2efe0cc8ac836093e) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A38%3A48.432Z&to=2026-11-16T09%3A43%3A48.499Z&oql=body%3A+%22dbde9e38ecad8ff2efe0cc8ac836093e%22+and+NOT+body%3A+%22request_data%22) |
| `7a9028e26e13df9f36438e3abcc8d222` | 2026-11-16T10:18:42 | `_4:504>_1:0` → **499** | 120047ms | fw: `context deadline exceeded`<br>conn reused=true idle=4922ms<br>时间线：响应头 未发出ms → Dynamo 见断开 2281ms；网关比 Dynamo 晚 117766ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=2972693B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/7a9028e26e13df9f36438e3abcc8d222) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T10%3A17%3A42.154Z&to=2026-11-16T10%3A22%3A42.201Z&oql=body%3A+%227a9028e26e13df9f36438e3abcc8d222%22+and+NOT+body%3A+%22request_data%22) |
| `d464e2d44a070c67733f4a42de68d92e` | 2026-11-16T15:36:22 | `_4:504>_1:200` → **200** | 120017ms | fw: `context deadline exceeded`<br>conn reused=true idle=0ms<br>时间线：响应头 1585ms → Dynamo 见断开 1794ms；网关比 Dynamo 晚 118222ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=627730B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/d464e2d44a070c67733f4a42de68d92e) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T15%3A35%3A22.139Z&to=2026-11-16T15%3A40%3A22.156Z&oql=body%3A+%22d464e2d44a070c67733f4a42de68d92e%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/B3.txt`

#### B4 拨号 / DNS 超时（4 个）

**证据**：`dial tcp 18.61.206.186:32027: i/o timeout`、`dial tcp 16.112.141.252:32027: i/o timeout`、`dial tcp: lookup rke2-aps2-svc-58f72dd1ead99337.elb.ap-south-2.amazonaws.com: i/o timeout`，均为 5s 后放弃，且全部重试 `_1` 成功。

**修复**：量很小。可以考虑给 NLB 域名做 DNS 缓存，并关注跨云链路质量。


**例子（B4）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `1eb3f44c62dda274572b4968a896e27a` | 2026-11-16T08:27:02 | `_4:504>_1:200` → **200** | 5027ms | fw: `dial tcp 16.112.141.252:32027: i/o timeout`<br>**无 Dynamo span**<br>body=860815B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/1eb3f44c62dda274572b4968a896e27a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A26%3A02.159Z&to=2026-11-16T08%3A29%3A07.186Z&oql=body%3A+%221eb3f44c62dda274572b4968a896e27a%22+and+NOT+body%3A+%22request_data%22) |
| `878585fda5fa57a8c852480957f0c544` | 2026-11-16T09:44:08 | `_4:504>_1:200` → **200** | 5301ms | fw: `dial tcp: lookup rke2-aps2-svc-58f72dd1ead99337.elb.ap-south-2.amazonaws.com: i/o timeout`<br>**无 Dynamo span**<br>body=3631462B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/878585fda5fa57a8c852480957f0c544) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A43%3A08.906Z&to=2026-11-16T09%3A46%3A14.207Z&oql=body%3A+%22878585fda5fa57a8c852480957f0c544%22+and+NOT+body%3A+%22request_data%22) |
| `5be0f9bb197ea59aa0e3a28c25028df2` | 2026-11-16T11:33:18 | `_4:504>_1:200` → **200** | 5002ms | fw: `dial tcp 18.61.206.186:32027: i/o timeout`<br>**无 Dynamo span**<br>body=6095B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/5be0f9bb197ea59aa0e3a28c25028df2) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T11%3A32%3A18.585Z&to=2026-11-16T11%3A35%3A23.587Z&oql=body%3A+%225be0f9bb197ea59aa0e3a28c25028df2%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/B4.txt`

### C. 504 流在 120s 内无任何内容（`error.type="timeout: Backend stream timed out before returning any content"`）

共 142 个。**修订**：之前判断为“Dynamo 内部失败后不关闭流”，证据不支持。实际情况是：Dynamo 发出 200 响应头后 0.25–4.5s 内就看到连接断开（`stream_drop`），随后才取消 decode，prefill 才报 KVTransferError。网关侧一直没收到断开信号，等满 120s。

#### C1 Dynamo 已收到请求后连接中断，网关 120s 无内容（93 个；KVTransferError 是副作用）

**证据**：93 个全部是 decode 路由 `cancelled / stream_drop`。响应头发出到 Dynamo 看到断开：p50 830ms，p90 4.5s。网关结束时间比 Dynamo 晚 p50 118.2s，没有一个在 1s 以内。典型时间线见 2.4 节（trace `2a6fc286…`）。

**结论**：链路问题。


**例子（C1）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `2a6fc286ab0fa11175ca8762b2d3a952` | 2026-11-16T04:36:02 | `_4:504>_1:200` → **200** | 120019ms | conn reused=true idle=21966ms<br>时间线：响应头 473ms → Dynamo 见断开 1701ms → prefill KV 报错 3294ms；网关比 Dynamo 晚 118318ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=658304B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/2a6fc286ab0fa11175ca8762b2d3a952) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T04%3A35%3A02.443Z&to=2026-11-16T04%3A40%3A02.462Z&oql=body%3A+%222a6fc286ab0fa11175ca8762b2d3a952%22+and+NOT+body%3A+%22request_data%22) |
| `df1bfba8d8b0f2d68ace1d1f435b2c4e` | 2026-11-16T12:01:04 | `_4:504>_1:0` → **499** | 120047ms | conn reused=true idle=14357ms<br>时间线：响应头 1929ms → Dynamo 见断开 7010ms → prefill KV 报错 13603ms；网关比 Dynamo 晚 113037ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=3012991B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/df1bfba8d8b0f2d68ace1d1f435b2c4e) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T12%3A00%3A04.500Z&to=2026-11-16T12%3A05%3A04.547Z&oql=body%3A+%22df1bfba8d8b0f2d68ace1d1f435b2c4e%22+and+NOT+body%3A+%22request_data%22) |
| `3c788ca5299a2820250a4994063fea49` | 2026-11-16T14:45:34 | `_4:504>_1:200` → **200** | 120021ms | conn reused=false idle=0ms<br>时间线：响应头 1133ms → Dynamo 见断开 12223ms → prefill KV 报错 12292ms；网关比 Dynamo 晚 107798ms 结束<br>prefill（副作用）: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an`<br>body=470485B org=43422337… | [trace](https://ie-log.gmi-serving.com/trace/3c788ca5299a2820250a4994063fea49) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A44%3A34.200Z&to=2026-11-16T14%3A49%3A34.221Z&oql=body%3A+%223c788ca5299a2820250a4994063fea49%22+and+NOT+body%3A+%22request_data%22) |
| `bb253ae40215a430afe92440c2b6b452` | 2026-11-16T20:59:19 | `_4:504>_1:0` → **499** | 120048ms | conn reused=true idle=462ms<br>时间线：响应头 1278ms → Dynamo 见断开 1872ms → prefill KV 报错 2074ms；网关比 Dynamo 晚 118176ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=3410833B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/bb253ae40215a430afe92440c2b6b452) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T20%3A58%3A19.578Z&to=2026-11-16T21%3A03%3A19.626Z&oql=body%3A+%22bb253ae40215a430afe92440c2b6b452%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/C1.txt`

#### C2 同 C1，但没采到 KV 报错（49 个）

**证据**：43 个 `stream_drop`。另有 6 个 decode 已正常完成，也就是 Dynamo 已经把 token 发出去了，但**网关一个字节都没收到**。网关结束时间比 Dynamo 晚 p50 118.7s。

**结论**：链路问题。


**例子（C2）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `56df33c1080ee4c5ecddbd574260addd` | 2026-11-16T08:26:09 | `_4:504>_1:200` → **200** | 120006ms | conn reused=false idle=0ms<br>时间线：响应头 1426ms → Dynamo 见断开 2043ms；网关比 Dynamo 晚 117963ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=163706B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/56df33c1080ee4c5ecddbd574260addd) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A25%3A09.163Z&to=2026-11-16T08%3A30%3A09.169Z&oql=body%3A+%2256df33c1080ee4c5ecddbd574260addd%22+and+NOT+body%3A+%22request_data%22) |
| `4969d649eac914d51855ba2c5ffe6e44` | 2026-11-16T14:10:21 | `_4:504>_1:200` → **200** | 120007ms | conn reused=true idle=15089ms<br>时间线：响应头 188ms → Dynamo 见断开 1017ms；网关比 Dynamo 晚 118990ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=275812B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/4969d649eac914d51855ba2c5ffe6e44) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A09%3A21.875Z&to=2026-11-16T14%3A14%3A21.882Z&oql=body%3A+%224969d649eac914d51855ba2c5ffe6e44%22+and+NOT+body%3A+%22request_data%22) |
| `a21fae9c9e2b879d2ae0e9740a63a973` | 2026-11-16T20:02:09 | `_4:504>_1:200` → **200** | 120005ms | conn reused=true idle=3934ms<br>时间线：响应头 582ms → Dynamo 见断开 1149ms；网关比 Dynamo 晚 118856ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=163752B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/a21fae9c9e2b879d2ae0e9740a63a973) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T20%3A01%3A09.051Z&to=2026-11-16T20%3A06%3A09.056Z&oql=body%3A+%22a21fae9c9e2b879d2ae0e9740a63a973%22+and+NOT+body%3A+%22request_data%22) |
| `d9d05fdf679a9b063e5f36542cee4633` | 2026-11-16T22:23:41 | `_4:504>_1:0` → **499** | 120230ms | conn reused=true idle=429ms<br>时间线：响应头 3437ms → Dynamo 见断开 3960ms；网关比 Dynamo 晚 116270ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=14808805B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/d9d05fdf679a9b063e5f36542cee4633) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T22%3A22%3A41.438Z&to=2026-11-16T22%3A27%3A41.668Z&oql=body%3A+%22d9d05fdf679a9b063e5f36542cee4633%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/C2.txt`

### D. 502 上游不可用（`error.type="backend_error: Upstream service unavailable"`）

共 146 个，绝大多数重试 `_1` 后成功。D1 是请求还没到 Dynamo，复用连接就被 RST；D2/D3 是 Dynamo 收到请求后连接断开，网关在约 100ms 内感知（快速 502，然后重试）。

#### D1 复用连接被对端 RST（124 个）

**证据**：forward-worker `error=read tcp 10.100.x.x:port->18.61.206.186:32027 (或 16.112.141.252:32027): read: connection reset by peer`（少量为 `write: connection reset by peer`），`failure_kind=transport`。没有 Dynamo span。97% 为复用连接；idle 中位数为 0（刚被上一个请求释放），p90 = 68s。

**根因判断**：对端（NLB / Dynamo frontend）已经关闭了连接，而客户端仍在复用（keep-alive 竞争）。idle=0 的那部分很可能是上一个流以异常结束后，对端关掉了连接，但连接仍被放回了连接池。

**修复**：同 A1 第 1 点。另外，对于“写入阶段就被 RST、尚未收到任何响应字节”的请求，可以安全地在新连接上原地重试 `_4`（Go `http.Transport` 默认不会重试 POST）。另需检查 forward-worker 在流异常结束（例如 C 类）后，是否把连接错误地放回了池里。


**例子（D1）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `ad8ae6c517de3a426ca2c9980a2cc807` | 2026-11-16T03:44:04 | `_4:502>_1:200` → **200** | 635ms | fw: `read tcp 10.100.26.20:38952->18.61.206.186:32027: read: connection reset by peer`<br>conn reused=true idle=0ms<br>**无 Dynamo span**<br>body=13447B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/ad8ae6c517de3a426ca2c9980a2cc807) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A43%3A04.082Z&to=2026-11-16T03%3A46%3A04.717Z&oql=body%3A+%22ad8ae6c517de3a426ca2c9980a2cc807%22+and+NOT+body%3A+%22request_data%22) |
| `27119cb2c1a13953416cbb12a58c162f` | 2026-11-16T09:17:37 | `_4:502>_1:200` → **200** | 78449ms | fw: `read tcp 10.100.22.22:49482->18.61.206.186:32027: read: connection reset by peer`<br>conn reused=true idle=60775ms<br>**无 Dynamo span**<br>body=354140B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/27119cb2c1a13953416cbb12a58c162f) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A16%3A37.215Z&to=2026-11-16T09%3A20%3A55.664Z&oql=body%3A+%2227119cb2c1a13953416cbb12a58c162f%22+and+NOT+body%3A+%22request_data%22) |
| `4d2e77e6f15f61147a5c6406f72f6239` | 2026-11-16T15:22:41 | `_4:502>_1:200` → **200** | 112147ms | fw: `read tcp 10.100.24.19:47286->16.112.141.252:32027: read: connection reset by peer`<br>conn reused=true idle=57893ms<br>**无 Dynamo span**<br>body=169209B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/4d2e77e6f15f61147a5c6406f72f6239) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T15%3A21%3A41.771Z&to=2026-11-16T15%3A26%3A33.918Z&oql=body%3A+%224d2e77e6f15f61147a5c6406f72f6239%22+and+NOT+body%3A+%22request_data%22) |
| `df981e27e9e2c036914d447b0b255479` | 2026-11-16T21:11:48 | `_4:502>_1:200` → **200** | 5615ms | fw: `read tcp 10.100.22.18:56412->16.112.141.252:32027: read: connection reset by peer`<br>conn reused=true idle=46408ms<br>**无 Dynamo span**<br>body=13870B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/df981e27e9e2c036914d447b0b255479) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T21%3A10%3A48.758Z&to=2026-11-16T21%3A13%3A54.373Z&oql=body%3A+%22df981e27e9e2c036914d447b0b255479%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/D1.txt`

#### D2 Dynamo 已收到请求后连接中断，网关快速感知（9 个；KVTransferError 是副作用）

**证据**：网关结束时间只比 Dynamo 看到断开晚 p50 103ms（9/9 在 1s 内）。这说明这一次断开信号**确实传到了** forward-worker，和 C/B 类形成对照。

**结论**：链路问题；能快速失败，并且全部重试成功。


**例子（D2）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `f669b83cfc90cc60ce801771e1979522` | 2026-11-16T08:11:43 | `_4:502>_1:200` → **200** | 362ms | conn reused=true idle=0ms<br>时间线：响应头 125ms → Dynamo 见断开 256ms → prefill KV 报错 334ms；网关比 Dynamo 晚 106ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=39095B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/f669b83cfc90cc60ce801771e1979522) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A10%3A43.091Z&to=2026-11-16T08%3A13%3A43.453Z&oql=body%3A+%22f669b83cfc90cc60ce801771e1979522%22+and+NOT+body%3A+%22request_data%22) |
| `8abec0e740c2bd10a2b43e7050044c37` | 2026-11-16T16:05:02 | `_4:502>_1:200` → **200** | 1467ms | conn reused=true idle=0ms<br>时间线：响应头 1233ms → Dynamo 见断开 1363ms → prefill KV 报错 1450ms；网关比 Dynamo 晚 104ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=617589B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/8abec0e740c2bd10a2b43e7050044c37) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T16%3A04%3A02.441Z&to=2026-11-16T16%3A07%3A03.908Z&oql=body%3A+%228abec0e740c2bd10a2b43e7050044c37%22+and+NOT+body%3A+%22request_data%22) |
| `ed634b6806e055ce8905f84f8546e2fd` | 2026-11-16T19:18:00 | `_4:502>_1:200` → **200** | 340ms | conn reused=true idle=60ms<br>时间线：响应头 64ms → Dynamo 见断开 237ms → prefill KV 报错 496ms；网关比 Dynamo 晚 103ms 结束<br>prefill（副作用）: `Prefill transfer failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Failed due to an `<br>body=6351B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/ed634b6806e055ce8905f84f8546e2fd) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T19%3A17%3A00.932Z&to=2026-11-16T19%3A20%3A01.272Z&oql=body%3A+%22ed634b6806e055ce8905f84f8546e2fd%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/D2.txt`

#### D3 同 D2，但没采到 KV 报错（13 个）

**证据**：10 个 `stream_drop`，3 个 decode 正常完成。网关比 Dynamo 晚 p50 152ms。

**结论**：链路问题。


**例子（D3）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `678b3e689af97e90bcfa98f2dc359897` | 2026-11-16T05:27:36 | `_4:502>_1:200` → **200** | 757ms | conn reused=true idle=0ms<br>时间线：响应头 174ms → Dynamo 见断开 624ms；网关比 Dynamo 晚 133ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=83521B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/678b3e689af97e90bcfa98f2dc359897) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T05%3A26%3A36.600Z&to=2026-11-16T05%3A29%3A37.357Z&oql=body%3A+%22678b3e689af97e90bcfa98f2dc359897%22+and+NOT+body%3A+%22request_data%22) |
| `282e05b7917c36a69de62fe4fdf9dd82` | 2026-11-16T12:42:29 | `_4:502>_1:200` → **200** | 1261ms | conn reused=true idle=0ms<br>时间线：响应头 512ms → Dynamo 见断开 1049ms；网关比 Dynamo 晚 212ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=43995B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/282e05b7917c36a69de62fe4fdf9dd82) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T12%3A41%3A29.725Z&to=2026-11-16T12%3A44%3A30.986Z&oql=body%3A+%22282e05b7917c36a69de62fe4fdf9dd82%22+and+NOT+body%3A+%22request_data%22) |
| `c9b6866c49050f970b3f986e96c13641` | 2026-11-16T15:27:31 | `_4:502>_1:200` → **200** | 686ms | conn reused=true idle=32ms<br>时间线：响应头 110ms → Dynamo 见断开 537ms；网关比 Dynamo 晚 149ms 结束<br>dynamo: `cancelled:cancelled before completion`<br>body=9586B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/c9b6866c49050f970b3f986e96c13641) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T15%3A26%3A31.939Z&to=2026-11-16T15%3A29%3A32.625Z&oql=body%3A+%22c9b6866c49050f970b3f986e96c13641%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/D3.txt`

### E. 400：Dynamo frontend 在流开始前拒绝（forward-worker `gmi.vendor_error_type=unsupported_operation`，日志里有 `vendor_body`）

共 309 个。校验发生在 Dynamo frontend（`python3 -m dynamo.frontend`，Rust HTTP 服务）里，请求还没进入路由和 SGLang worker。**归因 Dynamo**：问题在于 frontend 的请求兼容范围和报错方式。

此外，网关把这些 400 统一标成 **`unsupported_operation/Bad Request`**，`error_class=upstream_fault`，导致真实原因不可见。这个问题另行处理（见第 3 节第 5 条）。

**通用修复**
- Dynamo frontend：返回 OpenAI 兼容的错误体（`type=invalid_request_error`，带 `code` / `param`，如 `context_length_exceeded`），不要只返回 `"Bad Request"`。
- 网关：按 vendor_body 映射错误类型，并把 message 透传给调用方。

#### E1 上下文超长（134 个）

**日志原文**（forward-worker）：`vendor_body:{"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1052243}}`
请求体约 3–4MB，主要来自 org `747d2d0f-…` 和 `65a6f011-…`。

**机制**
- frontend 预检（`preprocessor.rs` 的 `validate_requested_token_budget` / `prompt_overflow_error`）在两种情况下都会报这个错：prompt 本身 ≥ 上限，或 prompt + max_tokens > 上限。
- 上限来自 SGLang worker 发布的 `context_len − num_reserved_tokens`。
- 错误只带结构化的 `details`，没有公开 message，所以 message 回落为通用的 `"Invalid request"`，type 为 HTTP 原因短语 `"Bad Request"`。

1,048,576 是模型上限，这类请求本身无法放行。要修的是错误体：应该能被识别为 `context_length_exceeded`。


**例子（E1）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `103d176d989be5684cc803aabfb9af3a` | 2026-11-16T03:47:32 | `_4:400` → **400** | 2381ms | fw: `unsupported_operation`<br>conn reused=true idle=62108ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1052243}}`<br>body=3179143B org=747d2d0f… | [trace](https://ie-log.gmi-serving.com/trace/103d176d989be5684cc803aabfb9af3a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A46%3A32.583Z&to=2026-11-16T03%3A49%3A34.964Z&oql=body%3A+%22103d176d989be5684cc803aabfb9af3a%22+and+NOT+body%3A+%22request_data%22) |
| `7b7618a1442c54500c0bda890331ed07` | 2026-11-16T09:51:42 | `_4:400` → **400** | 2396ms | fw: `unsupported_operation`<br>conn reused=true idle=9049ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1048727}}`<br>body=3818546B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/7b7618a1442c54500c0bda890331ed07) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A50%3A42.064Z&to=2026-11-16T09%3A53%3A44.460Z&oql=body%3A+%227b7618a1442c54500c0bda890331ed07%22+and+NOT+body%3A+%22request_data%22) |
| `3134d6d6380923d88735bef8c921f14a` | 2026-11-16T12:35:26 | `_4:400` → **400** | 2381ms | fw: `unsupported_operation`<br>conn reused=true idle=18769ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1067208}}`<br>body=3885766B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/3134d6d6380923d88735bef8c921f14a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T12%3A34%3A26.832Z&to=2026-11-16T12%3A37%3A29.213Z&oql=body%3A+%223134d6d6380923d88735bef8c921f14a%22+and+NOT+body%3A+%22request_data%22) |
| `bf20e7b1982875ed2d31d10415c4b817` | 2026-11-16T14:15:24 | `_4:400` → **400** | 2466ms | fw: `unsupported_operation`<br>conn reused=true idle=7499ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1089888}}`<br>body=3966323B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/bf20e7b1982875ed2d31d10415c4b817) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A14%3A24.259Z&to=2026-11-16T14%3A17%3A26.725Z&oql=body%3A+%22bf20e7b1982875ed2d31d10415c4b817%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/E1.txt`

#### E2 tools schema / 函数名校验失败（57 个）

**日志原文（典型）**
- `Validation: Invalid schema for function 'flag_issue': In context=#/required, must include property 'summary'`（39 个）
- `Validation: Invalid schema for function 'submit_conversation_parts': In context=#/properties/intents/properties/value/uniqueItems, keyword is not supported in strict schemas`
- `Validation: Invalid schema for function 'house_view_research': In context=#, object schemas require additionalProperties: false`
- `Validation: Function at index 0 has an invalid name: "files.get_file". Only a-z, A-Z, 0-9, underscores, and dashes are allowed.`

**机制**
- 只有 `tools[].function.strict == true` 时，frontend 才按 OpenAI strict 子集校验（`strict_schema.rs`）。这是 Dynamo 1.6.0 有意引入的行为，目前没有开关。
- 函数名按字节只允许 `[A-Za-z0-9_-]`（`validate.rs`）。

调用方现在能做的只有去掉 `strict` 或改为 `false`。


**例子（E2）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `d0c197e65f1bc0c4ee8f43dc029ac71e` | 2026-11-16T08:47:44 | `_4:400` → **400** | 240ms | fw: `unsupported_operation`<br>conn reused=true idle=56260ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: Function at index 0 has an invalid name: \"adaudit.users.list\". Only a-z, A-Z, 0-9, underscores, and dashes are allowed.","type":"Bad Request","code":400}`<br>body=61900B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/d0c197e65f1bc0c4ee8f43dc029ac71e) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A46%3A44.183Z&to=2026-11-16T08%3A49%3A44.423Z&oql=body%3A+%22d0c197e65f1bc0c4ee8f43dc029ac71e%22+and+NOT+body%3A+%22request_data%22) |
| `852fcbf1b72e522ed22a45f410c814d7` | 2026-11-16T11:02:12 | `_4:400` → **400** | 227ms | fw: `unsupported_operation`<br>conn reused=true idle=15863ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: Function at index 0 has an invalid name: \"files.get_file\". Only a-z, A-Z, 0-9, underscores, and dashes are allowed.","type":"Bad Request","code":400}`<br>body=34186B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/852fcbf1b72e522ed22a45f410c814d7) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T11%3A01%3A12.858Z&to=2026-11-16T11%3A04%3A13.085Z&oql=body%3A+%22852fcbf1b72e522ed22a45f410c814d7%22+and+NOT+body%3A+%22request_data%22) |
| `e817afc6478ebe2422df834d6b7c8e99` | 2026-11-16T14:00:10 | `_4:400` → **400** | 717ms | fw: `unsupported_operation`<br>conn reused=false idle=0ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: Invalid schema for function 'submit_conversation_parts': In context=#/properties/intents/properties/value/uniqueItems, keyword is not supported in strict schemas","type":"Bad Request","code":400}`<br>body=26472B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/e817afc6478ebe2422df834d6b7c8e99) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T13%3A59%3A10.207Z&to=2026-11-16T14%3A02%3A10.924Z&oql=body%3A+%22e817afc6478ebe2422df834d6b7c8e99%22+and+NOT+body%3A+%22request_data%22) |
| `5ebff0bcb88508894c383a121a2a27b3` | 2026-11-16T14:30:58 | `_4:400` → **400** | 216ms | fw: `unsupported_operation`<br>conn reused=true idle=8722ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: Invalid schema for function 'flag_issue': In context=#/required, must include property 'summary'","type":"Bad Request","code":400}`<br>body=2501B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/5ebff0bcb88508894c383a121a2a27b3) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A29%3A58.536Z&to=2026-11-16T14%3A32%3A58.752Z&oql=body%3A+%225ebff0bcb88508894c383a121a2a27b3%22+and+NOT+body%3A+%22request_data%22) |
| `d370b9f4778ccb7ab01b8daa1358e3ad` | 2026-11-16T16:15:09 | `_4:400` → **400** | 222ms | fw: `unsupported_operation`<br>conn reused=true idle=153ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: Invalid schema for function 'submit_conversation_commitments': In context=#/properties/questions/properties/unanswered/properties/value/uniqueItems, keyword is not supported in strict schemas","ty`<br>body=29183B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/d370b9f4778ccb7ab01b8daa1358e3ad) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T16%3A14%3A09.975Z&to=2026-11-16T16%3A17%3A10.197Z&oql=body%3A+%22d370b9f4778ccb7ab01b8daa1358e3ad%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/E2.txt`

#### E3 请求体反序列化失败：不支持的字段或枚举值（49 个）

**日志原文（典型）**
- `unknown variant 'openrouter:web_search', expected 'function'`（tools 中带了 OpenRouter 内置工具）
- `unknown variant 'tool_call', expected 'function'`
- `unknown variant 'mid', expected one of … minimal, low, medium, high, xhigh, max`（reasoning effort）
- `data did not match any variant of untagged enum ChatCompletionRequestUserMessageContent / SystemMessageContent`
- `duplicate field 'reasoning_content'`、`missing field 'type'`、`"json_object", expected internally tagged enum ResponseFormat`
- `Failed to convert responses request: Unsupported Responses tools type 'web_search'`

**修复（Dynamo frontend）**：frontend 的 serde 请求类型过严，一个不认识的字段或枚举值就会导致整个请求失败。建议改为宽松反序列化：忽略非 `function` 类型的 tool 并记录 warning；把 `tool_call` 当作 `function`；`reasoning.effort` 加别名（`mid` → `medium`）；接受字符串形式的 `response_format: "json_object"`；遇到重复的 `reasoning_content` 取最后一个；缺少 `type` 时按内容推断。


**例子（E3）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `60ec25fa13166dfa612e1a6c0a46889a` | 2026-11-16T03:47:37 | `_4:400` → **400** | 131ms | fw: `unsupported_operation`<br>conn reused=true idle=74735ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Failed to convert responses request: Unsupported Responses tools type 'web_search': the Chat Completions adapter supports only function tools and [redacted], auto, required, named function, or function-only a`<br>body=188B org=ac97f06c… | [trace](https://ie-log.gmi-serving.com/trace/60ec25fa13166dfa612e1a6c0a46889a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A46%3A37.723Z&to=2026-11-16T03%3A49%3A37.854Z&oql=body%3A+%2260ec25fa13166dfa612e1a6c0a46889a%22+and+NOT+body%3A+%22request_data%22) |
| `9680a1fb8d0348360074c45edc4ba169` | 2026-11-16T04:35:09 | `_4:400` → **400** | 1195ms | fw: `unsupported_operation`<br>conn reused=true idle=1847ms<br>vendor_body: `{"message":"Failed to deserialize the JSON body into the target type: data did not match any variant of untagged enum ChatCompletionRequestUserMessageContent at line 1 column 9612701","type":"Bad Request","code":400}`<br>body=9612644B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/9680a1fb8d0348360074c45edc4ba169) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T04%3A34%3A09.265Z&to=2026-11-16T04%3A37%3A10.460Z&oql=body%3A+%229680a1fb8d0348360074c45edc4ba169%22+and+NOT+body%3A+%22request_data%22) |
| `bc709e5185ee7b5deb91b23d40ea5435` | 2026-11-16T04:54:59 | `_4:400` → **400** | 136ms | fw: `unsupported_operation`<br>conn reused=true idle=8887ms<br>vendor_body: `{"message":"Failed to deserialize the JSON body into the target type: duplicate field `reasoning_content` at line 1 column 56929","type":"Bad Request","code":400}`<br>body=56891B org=e3e55b99… | [trace](https://ie-log.gmi-serving.com/trace/bc709e5185ee7b5deb91b23d40ea5435) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T04%3A53%3A59.511Z&to=2026-11-16T04%3A56%3A59.647Z&oql=body%3A+%22bc709e5185ee7b5deb91b23d40ea5435%22+and+NOT+body%3A+%22request_data%22) |
| `e555084858f4bbd9ee0c70bae6c003f0` | 2026-11-16T05:37:34 | `_4:400` → **400** | 137ms | fw: `unsupported_operation`<br>conn reused=true idle=40127ms<br>vendor_body: `{"message":"Failed to deserialize the JSON body into the target type: invalid type: string \"json_object\", expected internally tagged enum ResponseFormat at line 1 column 329","type":"Bad Request","code":400}`<br>body=310B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/e555084858f4bbd9ee0c70bae6c003f0) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T05%3A36%3A34.268Z&to=2026-11-16T05%3A39%3A34.405Z&oql=body%3A+%22e555084858f4bbd9ee0c70bae6c003f0%22+and+NOT+body%3A+%22request_data%22) |
| `025169166fabd81987dc8e586811e383` | 2026-11-16T06:25:47 | `_4:400` → **400** | 260ms | fw: `unsupported_operation`<br>conn reused=true idle=69119ms<br>vendor_body: `{"message":"Failed to deserialize the JSON body into the target type: missing field `type` at line 1 column 410233","type":"Bad Request","code":400}`<br>body=410159B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/025169166fabd81987dc8e586811e383) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T06%3A24%3A47.363Z&to=2026-11-16T06%3A27%3A47.623Z&oql=body%3A+%22025169166fabd81987dc8e586811e383%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/E3.txt`

#### E4 历史 `tool_calls[].function.arguments` 不是合法的 JSON 字符串（22 个）

**日志原文**：`Validation: 'messages[162].tool_calls[0].function.arguments' must be a valid JSON object string: control character (\u0000-\u001F) found …` / `trailing characters` / `EOF while parsing a string`

**修复（Dynamo frontend）**：frontend 对**历史消息**里的 arguments 也做了严格 JSON 校验，但这些只是上下文，不影响本次生成。建议放宽：校验失败时把原字符串直接交给 chat template，或包装为 `{"_raw": "..."}`，只记录 warning。


**例子（E4）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `01ff39704358b20c34d3acfd3d81b9a9` | 2026-11-16T14:12:26 | `_4:400` → **400** | 221ms | fw: `unsupported_operation`<br>conn reused=true idle=3063ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: `messages[2].tool_calls[0].function.arguments` must be a valid JSON object string","type":"Bad Request","code":400}`<br>body=51778B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/01ff39704358b20c34d3acfd3d81b9a9) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T14%3A11%3A26.000Z&to=2026-11-16T14%3A14%3A26.221Z&oql=body%3A+%2201ff39704358b20c34d3acfd3d81b9a9%22+and+NOT+body%3A+%22request_data%22) |
| `c63d66b3474a764f3df7a1da5ef415b4` | 2026-11-17T00:17:04 | `_4:400` → **400** | 219ms | fw: `unsupported_operation`<br>conn reused=true idle=72326ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: `messages[2].tool_calls[0].function.arguments` must be a valid JSON object string: EOF while parsing a string at line 1 column 476","type":"Bad Request","code":400}`<br>body=23524B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/c63d66b3474a764f3df7a1da5ef415b4) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-17T00%3A16%3A04.522Z&to=2026-11-17T00%3A19%3A04.741Z&oql=body%3A+%22c63d66b3474a764f3df7a1da5ef415b4%22+and+NOT+body%3A+%22request_data%22) |
| `414672d9eb628dab7086dba30c4ca29d` | 2026-11-17T01:01:13 | `_4:400` → **400** | 310ms | fw: `unsupported_operation`<br>conn reused=true idle=6349ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: `messages[20].tool_calls[0].function.arguments` must be a valid JSON object string: EOF while parsing an object at line 1 column 378","type":"Bad Request","code":400}`<br>body=693878B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/414672d9eb628dab7086dba30c4ca29d) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-17T01%3A00%3A13.049Z&to=2026-11-17T01%3A03%3A13.359Z&oql=body%3A+%22414672d9eb628dab7086dba30c4ca29d%22+and+NOT+body%3A+%22request_data%22) |
| `61feff0aade76b4a38ac179a4ab9352a` | 2026-11-17T02:08:09 | `_4:400` → **400** | 1391ms | fw: `unsupported_operation`<br>conn reused=false idle=0ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: `messages[162].tool_calls[0].function.arguments` must be a valid JSON object string: control character (\\u0000-\\u001F) found while parsing a string at line 1 column 661","type":"Bad Request","co`<br>body=217775B org=78e47bca… | [trace](https://ie-log.gmi-serving.com/trace/61feff0aade76b4a38ac179a4ab9352a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-17T02%3A07%3A09.533Z&to=2026-11-17T02%3A10%3A10.924Z&oql=body%3A+%2261feff0aade76b4a38ac179a4ab9352a%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/E4.txt`

#### E5 `Invalid request`，没有任何细节（46 个）

**日志原文**：vendor_body 只有 `{"message":"Invalid request","type":"Bad Request","code":400}`；Dynamo frontend 只记录了 `error_type=validation error_detail="invalid request parameters"`，elapsed 0–1ms。

**机制**
- frontend 内部把真实原因放在“私有诊断”里：`invalid_argument_error` 生成的错误不带公开 message。返回给调用方时，message 就回落为 `"Invalid request"`。
- 真实原因**只在 debug 级别**记录（`openai.rs` 中的 `Semantic request failure`）。
- 候选来源：chat template 渲染失败（`map_prompt_render_error`）、tool_choice / structural tag 相关错误、媒体 URL 策略等。

**怎么看到原因**：不改代码时，可以给 frontend 设置 `DYN_LOG="info,dynamo_llm::http::service::openai=debug,dynamo_llm::preprocessor=debug"`。代码修复见 `FIX_PLAN.md`。


**例子（E5）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `a04ccc00221867e5389d400864bf2285` | 2026-11-16T08:28:30 | `_4:400` → **400** | 234ms | fw: `unsupported_operation`<br>conn reused=true idle=17621ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400}`<br>body=32322B org=45af1aae… | [trace](https://ie-log.gmi-serving.com/trace/a04ccc00221867e5389d400864bf2285) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A27%3A30.024Z&to=2026-11-16T08%3A30%3A30.258Z&oql=body%3A+%22a04ccc00221867e5389d400864bf2285%22+and+NOT+body%3A+%22request_data%22) |
| `73cdbe6dbe1dae694bbc2b0ff7fb1c0f` | 2026-11-16T18:06:20 | `_4:400` → **400** | 670ms | fw: `unsupported_operation`<br>conn reused=false idle=0ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400}`<br>body=16346B org=f11fafcf… | [trace](https://ie-log.gmi-serving.com/trace/73cdbe6dbe1dae694bbc2b0ff7fb1c0f) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T18%3A05%3A20.077Z&to=2026-11-16T18%3A08%3A20.747Z&oql=body%3A+%2273cdbe6dbe1dae694bbc2b0ff7fb1c0f%22+and+NOT+body%3A+%22request_data%22) |
| `c1aa0dc5c696b46bc2e4c449bcc2b924` | 2026-11-16T23:03:34 | `_4:400` → **400** | 259ms | fw: `unsupported_operation`<br>conn reused=true idle=509ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400}`<br>body=1989846B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/c1aa0dc5c696b46bc2e4c449bcc2b924) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T23%3A02%3A34.287Z&to=2026-11-16T23%3A05%3A34.546Z&oql=body%3A+%22c1aa0dc5c696b46bc2e4c449bcc2b924%22+and+NOT+body%3A+%22request_data%22) |
| `a12bc3b031dded4cf2a99199974f565a` | 2026-11-16T23:47:46 | `_4:400` → **400** | 295ms | fw: `unsupported_operation`<br>conn reused=true idle=1887ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Invalid request","type":"Bad Request","code":400}`<br>body=2228683B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/a12bc3b031dded4cf2a99199974f565a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T23%3A46%3A46.736Z&to=2026-11-16T23%3A49%3A47.031Z&oql=body%3A+%22a12bc3b031dded4cf2a99199974f565a%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/E5.txt`

#### E6 `thinking` 参数格式（1 个）

**日志原文**：`Validation: 'thinking' must be a boolean or an object with 'type' set to 'enabled', 'disabled', or 'adaptive'`

**修复（Dynamo frontend）**：放宽 `thinking` 的解析，遇到不认识的写法时忽略并记录 warning，不要拒绝整个请求。


**例子（E6）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `3899560a68096ccfd9f7cfe0846a7a3a` | 2026-11-17T03:20:18 | `_4:400` → **400** | 522ms | fw: `unsupported_operation`<br>conn reused=false idle=0ms<br>dynamo: `validation:invalid request parameters`<br>vendor_body: `{"message":"Validation: `thinking` must be a boolean or an object with `type` set to `enabled`, `disabled`, or `adaptive`","type":"Bad Request","code":400}`<br>body=5544B org=be331962… | [trace](https://ie-log.gmi-serving.com/trace/3899560a68096ccfd9f7cfe0846a7a3a) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-17T03%3A19%3A18.200Z&to=2026-11-17T03%3A22%3A18.722Z&oql=body%3A+%223899560a68096ccfd9f7cfe0846a7a3a%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/E6.txt`

### F. 400：Dynamo 已返回 200 头，SGLang worker 处理时流内报错（`error.type="vendor_error_frame/invalid_request/400"`）

共 302 个。**归因 Dynamo。**

frontend 的 pre-commit 检查默认关闭（`DYN_HTTP_PRE_COMMIT_ERROR_PEEK_MS` 未设置，即 `BackendErrorCheck::Skip`），所以请求一进来就提交了 HTTP 200。SGLang worker 稍后（通常不到 1s）在加载图片或编译 grammar 时报错，这时只能以流内错误帧返回。日志中没有 vendor_body，真实原因只在 Dynamo span 的 `reason` 和 Dynamo 日志的 `attributes.error` 里。

#### F1 `response_format` 的 JSON Schema 被 SGLang 的 xgrammar 预校验拒绝（144 个，全部来自同一个 org）

**Dynamo prefill / decode span `reason` 原文（典型）**
- `Failed to compile json grammar: JSON Schema at #/properties/meanings/items/properties/correctsVersionId/anyOf/0 is not supported by xgrammar: constraints format, pattern cannot be enforced together`（40 个）
- `… at #/properties/…/alternatives is not supported by xgrammar: keyword(s) uniqueItems would be ignored`
- `… at #/properties/issue_key … constraints pattern, minLength/maxLength cannot be enforced together`
- `… at #/allOf/0 … keyword(s) if, then would be ignored`
- `json_schema_converter.cc: maxLength must be an integer`

**机制**
- SGLang v0.5.23 的 xgrammar 后端在编译前会无条件调用 `validate_xgrammar_json_schema`（`sglang/srt/constrained/json_schema_validation.py`，调用点 `xgrammar_backend.py` `dispatch_json`）。它主动拒绝 xgrammar 0.2.x 会静默忽略的约束：`uniqueItems`、`if/then/else`、`not`、`contains`、`multipleOf` 等，以及 `format+pattern`、`pattern+minLength/maxLength` 这类无法同时强制的组合。prefill 和 decode 两侧都会编译，都会以 400 中止请求。
- Dynamo frontend 原样透传 `response_format.json_schema.schema`（`chat_completions.rs` `get_guided_json`），**不读取 `strict`**。
- `maxLength must be an integer` 是另一个问题：xgrammar 自身要求这类字段必须是整数，例如 `100.0` 这样的浮点数就会报错。它与上面的预校验无关。


**例子（F1）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `241680d87b25e1c345d812c25b3468a7` | 2026-11-16T04:00:42 | `_4:400` → **400** | 261ms | conn reused=true idle=49786ms<br>dynamo: `Failed to compile json grammar: JSON Schema at #/properties/findings/items/properties/evidence_ids is not supported by xgrammar: keyword(s) uniqueItem`<br>body=6247B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/241680d87b25e1c345d812c25b3468a7) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A59%3A42.077Z&to=2026-11-16T04%3A02%3A42.338Z&oql=body%3A+%22241680d87b25e1c345d812c25b3468a7%22+and+NOT+body%3A+%22request_data%22) |
| `9027ff064e333b2f671427da2cec83ae` | 2026-11-16T05:14:52 | `_4:400` → **400** | 442ms | conn reused=true idle=68070ms<br>dynamo: `Failed to compile json grammar: JSON Schema at #/properties/issue_key is not supported by xgrammar: constraints pattern, minLength/maxLength cannot be`<br>body=12778B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/9027ff064e333b2f671427da2cec83ae) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T05%3A13%3A52.977Z&to=2026-11-16T05%3A16%3A53.419Z&oql=body%3A+%229027ff064e333b2f671427da2cec83ae%22+and+NOT+body%3A+%22request_data%22) |
| `17719ad977b69f13c24b8b064e0a5e8c` | 2026-11-16T06:14:44 | `_4:400` → **400** | 479ms | conn reused=true idle=1122ms<br>dynamo: `Failed to compile json grammar: JSON Schema at #/properties/observations/items/properties/id is not supported by xgrammar: constraints pattern, minLen`<br>body=79676B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/17719ad977b69f13c24b8b064e0a5e8c) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T06%3A13%3A44.068Z&to=2026-11-16T06%3A16%3A44.547Z&oql=body%3A+%2217719ad977b69f13c24b8b064e0a5e8c%22+and+NOT+body%3A+%22request_data%22) |
| `4c44c0406b7379c12ab846f8f41ccea0` | 2026-11-16T06:30:32 | `_4:400` → **400** | 496ms | conn reused=true idle=43452ms<br>dynamo: `Failed to compile json grammar: JSON Schema at #/properties/annotation/properties/units/items/allOf/0 is not supported by xgrammar: keyword(s) if, the`<br>body=138957B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/4c44c0406b7379c12ab846f8f41ccea0) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T06%3A29%3A32.339Z&to=2026-11-16T06%3A32%3A32.835Z&oql=body%3A+%224c44c0406b7379c12ab846f8f41ccea0%22+and+NOT+body%3A+%22request_data%22) |
| `07055b051a6d56a95f5af467c229d2cc` | 2026-11-16T07:59:30 | `_4:400` → **400** | 536ms | conn reused=true idle=26182ms<br>dynamo: `Failed to compile json grammar: JSON Schema at #/allOf/0 is not supported by xgrammar: keyword(s) if, then would be ignored`<br>body=209179B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/07055b051a6d56a95f5af467c229d2cc) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T07%3A58%3A30.477Z&to=2026-11-16T08%3A01%3A31.013Z&oql=body%3A+%2207055b051a6d56a95f5af467c229d2cc%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/F1.txt`

#### F2 非图片媒体经 `image_url` 进入图片加载器（125 个，涉及 49 个 org，影响面最广）

**Dynamo 日志原文**
- `mm-routing: failed to fetch image dims; MM routing entry skipped  error="fetch_image_dims failed: The image format could not be determined" url="data:audio/wav;base64"`
- `Prefill background task failed  error="Prefill execution failed: … InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index 0: Error while loading data data:application/pdf;base64"`、`… "Could not decode image: cannot identify image file"`

分布：`audio/wav` 92 个，`application/pdf` 32 个，`image/heic` 1 个。

**机制**
- 本部署走 URL 透传。frontend 既不校验 MIME，也不解码图片：它只为 mm-routing 读取图片尺寸，读取失败也只打一条 WARN。原始的 `data:` URI 会同时发给 prefill 和 decode，两侧各解码一次。
- SGLang `get_image_bytes`（`utils/common.py`）直接丢弃 MIME 前缀，把数据交给 Pillow。Pillow 识别失败后抛出 `ValueError`，最终变成 `InvalidArgument`。
- Dynamo 的 Python `ImageLoader` 其实已有“Data URL must be an image type”的检查，但 SGLang 分离式部署的路径不经过它。


**例子（F2）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `2d9f5a52379d7ce54260cfbcc718d257` | 2026-11-16T03:54:57 | `_4:400` → **400** | 473ms | conn reused=true idle=25633ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `or in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:application/pdf;base64`<br>body=116360B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/2d9f5a52379d7ce54260cfbcc718d257) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T03%3A53%3A57.274Z&to=2026-11-16T03%3A56%3A57.747Z&oql=body%3A+%222d9f5a52379d7ce54260cfbcc718d257%22+and+NOT+body%3A+%22request_data%22) |
| `0f27cd60d37b79270627dfbc0da744e9` | 2026-11-16T08:27:52 | `_4:400` → **400** | 747ms | conn reused=true idle=8518ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `ed error in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:audio/wav;base64`<br>body=32322B org=715c8f35… | [trace](https://ie-log.gmi-serving.com/trace/0f27cd60d37b79270627dfbc0da744e9) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T08%3A26%3A52.324Z&to=2026-11-16T08%3A29%3A53.071Z&oql=body%3A+%220f27cd60d37b79270627dfbc0da744e9%22+and+NOT+body%3A+%22request_data%22) |
| `569892ee9b4d2518fbe2709c3583b71b` | 2026-11-16T13:18:39 | `_4:400` → **400** | 730ms | conn reused=true idle=31583ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `d error in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:image/heic;base64`<br>body=2181467B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/569892ee9b4d2518fbe2709c3583b71b) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T13%3A17%3A39.371Z&to=2026-11-16T13%3A20%3A40.101Z&oql=body%3A+%22569892ee9b4d2518fbe2709c3583b71b%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/F2.txt`

#### F3 图片解码失败（png / jpeg / gif）（25 个）

**日志原文**：`Error while loading data data:image/png;base64`（另有 jpeg、gif），以及 `fetch_image_dims failed: The image format could not be determined`

**机制**：frontend 的 Rust `image` crate 也判断“格式无法确定”。这说明 base64 本身合法，但解出来的字节并不是声明的格式。可能是改了扩展名的 HEIC、SVG 或 HTML，需要记录解码后的前十几个字节才能确认。


**例子（F3）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `6af72a602d1a0e4ac8c7778e7ae10d60` | 2026-11-16T11:27:22 | `_4:400` → **400** | 432ms | conn reused=true idle=42856ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `ed error in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:image/gif;base64`<br>body=403B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/6af72a602d1a0e4ac8c7778e7ae10d60) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T11%3A26%3A22.757Z&to=2026-11-16T11%3A29%3A23.189Z&oql=body%3A+%226af72a602d1a0e4ac8c7778e7ae10d60%22+and+NOT+body%3A+%22request_data%22) |
| `d5d665870b414e99ba7ef8b5acd0e8a1` | 2026-11-16T15:11:50 | `_4:400` → **400** | 1630ms | conn reused=true idle=578ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `ed error in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:image/png;base64`<br>body=2436503B org=747d2d0f… | [trace](https://ie-log.gmi-serving.com/trace/d5d665870b414e99ba7ef8b5acd0e8a1) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T15%3A10%3A50.922Z&to=2026-11-16T15%3A13%3A52.552Z&oql=body%3A+%22d5d665870b414e99ba7ef8b5acd0e8a1%22+and+NOT+body%3A+%22request_data%22) |
| `9d2c9fd284d6d46b21f3b4be89dc4ff2` | 2026-11-16T16:41:21 | `_4:400` → **400** | 2023ms | conn reused=true idle=28959ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `d error in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:image/jpeg;base64`<br>body=3532268B org=747d2d0f… | [trace](https://ie-log.gmi-serving.com/trace/9d2c9fd284d6d46b21f3b4be89dc4ff2) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T16%3A40%3A21.906Z&to=2026-11-16T16%3A43%3A23.929Z&oql=body%3A+%229d2c9fd284d6d46b21f3b4be89dc4ff2%22+and+NOT+body%3A+%22request_data%22) |
| `4f0cd473642b80fb77f64577add3a9f0` | 2026-11-16T19:43:52 | `_4:400` → **400** | 1800ms | conn reused=true idle=74916ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `ed error in output stream: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index N: Error while loading data data:image/png;base64`<br>body=750607B org=747d2d0f… | [trace](https://ie-log.gmi-serving.com/trace/4f0cd473642b80fb77f64577add3a9f0) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T19%3A42%3A52.960Z&to=2026-11-16T19%3A45%3A54.760Z&oql=body%3A+%224f0cd473642b80fb77f64577add3a9f0%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/F3.txt`

#### F4 prefill 时上下文超长（3 个）

**日志原文**：`InvalidRequest: ValueError: The input (1572413 tokens) is longer than the model's context length (1048576 tokens).`

**机制**：frontend 的长度预检对**含图片的请求直接跳过**。`exact_prompt_len`（`preprocessor.rs:1849`）在有图片、但拿不到 MM 展开长度时返回 None；而 DeepSeek-V4.1 不在 MM routing 的 prompt layout 支持列表里，所以永远拿不到展开长度。

DeepSeek-V4.1 每张图最多 1024 token（`max_image_tokens`）。这些请求超出上限五十多万 token，说明**纯文本本身就已超限**。加一个下界检查（文本 token 数 ≥ 上限就拒绝）即可在 frontend 拦住。


**例子（F4）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `7f4f7595acfa5f53f0f8a929ba73d908` | 2026-11-16T12:17:32 | `_4:400` → **400** | 4104ms | conn reused=true idle=38046ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `fill router returned error in output stream: InvalidRequest: ValueError: The input (1572413 tokens) is longer than the model\'s context length (1048576 tokens).`<br>body=5979927B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/7f4f7595acfa5f53f0f8a929ba73d908) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T12%3A16%3A32.070Z&to=2026-11-16T12%3A19%3A36.174Z&oql=body%3A+%227f4f7595acfa5f53f0f8a929ba73d908%22+and+NOT+body%3A+%22request_data%22) |
| `6a52b1c27c3c6a2526c79cdeebe979f9` | 2026-11-16T18:29:40 | `_4:400` → **400** | 4003ms | conn reused=true idle=37598ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `fill router returned error in output stream: InvalidRequest: ValueError: The input (1234754 tokens) is longer than the model\'s context length (1048576 tokens).`<br>body=6527801B org=747d2d0f… | [trace](https://ie-log.gmi-serving.com/trace/6a52b1c27c3c6a2526c79cdeebe979f9) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T18%3A28%3A40.563Z&to=2026-11-16T18%3A31%3A44.566Z&oql=body%3A+%226a52b1c27c3c6a2526c79cdeebe979f9%22+and+NOT+body%3A+%22request_data%22) |
| `a15b98fde577a68bf53938df2f1d73eb` | 2026-11-16T20:04:29 | `_4:400` → **400** | 3667ms | conn reused=true idle=4833ms<br>dynamo: `validation:invalid request parameters`<br>dynamo log: `fill router returned error in output stream: InvalidRequest: ValueError: The input (1075278 tokens) is longer than the model\'s context length (1048576 tokens).`<br>body=4292595B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/a15b98fde577a68bf53938df2f1d73eb) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T20%3A03%3A29.362Z&to=2026-11-16T20%3A06%3A33.029Z&oql=body%3A+%22a15b98fde577a68bf53938df2f1d73eb%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/F4.txt`

#### F5 decode 以 InvalidArgument 拒绝多模态请求，prefill 空等 300s（5 个）

**证据（trace `113271f6…`）**
- 请求带 4 张图片。frontend 在约 1.35s 时发出 200 响应头。
- decode 收到请求约 140ms 后，路由以 `rejected / backend_invalid_argument` 结束；frontend 记 `validation / invalid request parameters`，调用方收到 400 错误帧。
- **decode 拒绝的具体原因没有任何日志**：`route_span.rs` 的 `finish_error` 只记录 `error.type`。
- prefill 因为等不到 decode 来 bootstrap，**300s 后**才超时（`SGLANG_DISAGGREGATION_BOOTSTRAP_TIMEOUT` 默认值为 300）：`Prefill bootstrap failed ... timed out after 300.0s in KVPoll.Bootstrapping`。frontend 的 http-request span 也因此持续约 301s。

**机制**
- frontend 的 `mm-routing: exact MM routing info not built` WARN 不是原因：DeepSeek-V4.1 的每个含图请求都会打这条。
- decode 侧专有的拒绝原因中，最可能的是**总 token 检查**：decode 使用调用方的 `max_tokens`，而 prefill 只用 1，所以“输入 + max_tokens 超过上下文”只会在 decode 侧触发。frontend 又恰好对含图请求跳过了这项检查（见 F4）。
- 其他可能：decode 的 KV 池比 prefill 小；decode 侧的多模态配置不同；只有 decode 侧取图失败。需要 decode worker 的日志才能确认。
- prefill 是独立 context 的后台任务（`prefill_router/handoff.rs`），decode 失败时**不会被取消**。这是为了保证调用方断开时不打断 KV 传输而有意设计的，但也导致 decode 已经拒绝时，prefill 依然空等到超时。

**结论（修订）**：KV bootstrap 超时是结果，不是原因。需要修的是：① 弄清并修复 decode 拒绝的原因；② decode 拒绝后主动取消配对的 prefill；③ 记录拒绝的具体原因。


**例子（F5）**

| trace_id | 时间(UTC) | 尝试链 → 客户端结果 | 耗时 | 关键信息 | 链接 |
|---|---|---|---:|---|---|
| `951e23b91b7d36b0b49c06ec2f324470` | 2026-11-16T09:06:47 | `_4:400` → **400** | 1979ms | conn reused=true idle=2559ms<br>dynamo: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Request <n> time`<br>body=1617237B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/951e23b91b7d36b0b49c06ec2f324470) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A05%3A47.423Z&to=2026-11-16T09%3A08%3A49.402Z&oql=body%3A+%22951e23b91b7d36b0b49c06ec2f324470%22+and+NOT+body%3A+%22request_data%22) |
| `113271f61ec960c549f070347c882543` | 2026-11-16T09:06:54 | `_4:400` → **400** | 1832ms | conn reused=true idle=15727ms<br>dynamo: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Request <n> time`<br>body=1620698B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/113271f61ec960c549f070347c882543) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T09%3A05%3A54.382Z&to=2026-11-16T09%3A08%3A56.214Z&oql=body%3A+%22113271f61ec960c549f070347c882543%22+and+NOT+body%3A+%22request_data%22) |
| `b5a6740eb3a24055b1bb95ba4ff05272` | 2026-11-16T12:17:42 | `_4:400` → **400** | 1812ms | conn reused=true idle=18985ms<br>dynamo: `Prefill bootstrap failed for request rank=<n> req.rid=<rid> req.bootstrap_room=<n> with exception KVTransferError(bootstrap_room=<n>: Request <n> time`<br>body=2390189B org=65a6f011… | [trace](https://ie-log.gmi-serving.com/trace/b5a6740eb3a24055b1bb95ba4ff05272) · [logs](https://ie-log.gmi-serving.com/service-logs?from=2026-11-16T12%3A16%3A42.876Z&to=2026-11-16T12%3A19%3A44.688Z&oql=body%3A+%22b5a6740eb3a24055b1bb95ba4ff05272%22+and+NOT+body%3A+%22request_data%22) |

完整列表：`out/trace_ids_by_category/F5.txt`


---

## 4. 修复计划

已单独整理到同目录的 **`FIX_PLAN.md`**，内容包括：每项问题在 Dynamo 代码中的位置和修复方法、配置类的快速修复、可观测性补强、链路验证步骤、上游 PR 拆分建议和验收方法。

---

## 5. 产出文件

- `out/failure_trace_ids.csv`：全部 3,644 个失败事件。列包括 trace_id、时间、子类、状态、重试链、错误文本、Dynamo 原因、vendor_body、org、请求体大小，以及新增的 frontend 响应头 / 断开时间、网关比 Dynamo 晚多少
- `out/trace_ids_by_category/<子类>.txt`：按子类拆分的 trace id 列表
- `out/g1g2_timeline_evidence.csv`：A2/A3/B2/B3/C*/D2/D3/F5 共 655 条 trace 的时间线证据
- `out/examples.json`：本报告中所有例子的结构化详情（含平台链接）
- `out/failure_events.json`：解析后的全量事件（含 Dynamo span 原因、日志解析字段）
- `out/raw/hNN.json`：按小时拉取的原始 trace JSON；`out/logs/<trace_id>.json`：已拉取的日志（全部 400，以及 5xx/499 的抽样）
- `FIX_PLAN.md`：修复计划

复现命令（时间窗与本报告一致）：

```bash
export GMI_SRE_BASE_URL=https://ie-log.gmi-serving.com
gmi-sre traces get <TRACE_ID> --format json -o ./out/trace-<TRACE_ID>.json
gmi-sre logs for-trace --trace-id <TRACE_ID> --from <例子时间-1m> --to <例子时间+5m> -o ./out/logs-<TRACE_ID>.json
# frontend 的断开日志不带 trace_id，需要按时间窗查：
gmi-sre logs oql -q 'body: "Stream closed unexpectedly" AND timestamp:[<start_ms> TO <end_ms>]' -o ./out/disconnect.json
```
