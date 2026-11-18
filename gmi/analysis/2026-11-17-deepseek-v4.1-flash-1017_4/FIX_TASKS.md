# Dynamo 修复任务清单：deepseek-v4.1-flash-1017_4

- **用途**：把 `FIX_PLAN.md` 拆成可以逐个建 task 跟踪的工作项。每张任务卡都有：代表性例子、修复前复现步骤和实际输出、修复要点、验收方法，以及 issue / PR 草稿。
- **与 FIX_PLAN 的关系**：FIX_PLAN 讲设计（为什么、改哪里）；本文件讲执行（怎么复现、怎么验收、怎么结案）。两者冲突时以本文件为准，更正汇总见第 5 节。
- **核实情况（2026-11-18）**：
  - 所有复现步骤都已实跑：本机 `ai-dynamo==1.6.1` frontend + mocker（L2），以及 xgrammar 0.2.7 / Pillow 13 的最小脚本（L1）。
  - 没有实跑的项在卡片里标了“代码推断”：F4、P0-2(e)，以及所有需要 GPU 的 L3 步骤。
  - 每项都对照了上游 main（本地 `main`，2026-11-16，`b20cd5007`），确认问题是否还在。
- **复现材料**：`out/fix_repro/`。环境准备和复现层级（L1–L4）的定义见 `out/fix_repro/README.md`。下文命令里的 `$M` 是模型名，`localhost:8000` 是 frontend 地址。
- **说明**：本文是 GMI fork 内部的分析记录。文中 `out/…` 开头的路径指分析工作区 `obs-console-playground` 中的数据与脚本，未随本文提交。文中的地址和客户标识已脱敏：NLB 的两个 IP 记为 NLB-IP-1、NLB-IP-2，NLB 主机名记为 `nlb.example`，控制台记为 `obs-console.example`，forward-worker 的 pod IP 记为 pod-A1 这样的代号（字母相同表示 /24 网段相同；两个区域的网段有重叠，区域也相同才是同一节点），集群名只保留区域，客户 org ID 已删除。

---

## 1. 任务总表

“去向”一列：
- **配置**：只改 dynamo-recipes 部署，不涉及代码。
- **fork**：只改 fork 分支，不提上游。
- **cherry-pick**：上游 main 已经修复，回合到 fork 即可。
- **上游 PR**：在上游新提 PR，合入后再回合到 fork。
- **先讨论**：与上游现有设计冲突，先开 issue 讨论，再写代码。

| ID | 任务 | 解决（次/天） | 去向 | 依赖 | 复现层级 | 批次 |
|---|---|---|---|---|---|---|
| T01 | 开启 pre-commit 错误窥探 | F1–F5 从 200+错误帧变为直接 400（302） | 配置 | — | L2 ✔ | 1 |
| T02 | E5 来源取证：临时打开 debug 日志一天 | 为 T19 定位（46） | 配置 | — | L2 ✔ | 1 |
| T03 | SSE keep-alive + 网关“无字节”超时 | C/B 类 120s 等待缩短到约 20s | 配置 + 网关 | T01 窗口 | L2 ✔ | 3 |
| T04 | 断开日志带 trace_id 和对端地址 | 链路取证 | 上游 PR | — | L2 ✔ | 2 |
| T05 | `route request failed` 记录错误消息 | F5 等原因可见 | 上游 PR | — | L2 ✔ | 2 |
| T06 | 校验失败原因记 WARN | E1–E6 原因进日志 | 上游 PR | — | L2 ✔ | 2 |
| T07 | SGLang prefill abort 上报 | F5 的 prefill 失败可见 | cherry-pick（58e83f6d2 的 Python 部分） | — | L1（代码推断） | 1 |
| T08 | 区分取消来源，降级 `Failed to publish complete final` | 去掉断开后的 ERROR 噪声 | 先讨论 → 上游 PR | — | L2 ✔ | 4 |
| T09 | 含图请求的长度下界检查 | F4 3 + F5 5 | 上游 PR | — | L1' / L3 | 2 |
| T10 | decode 拒绝后取消配对的 prefill | F5 prefill 空等 300s | 先讨论 → 上游 PR | T09 先上 | L2 ✔ | 4 |
| T11 | data URI 媒体格式嗅探 | F2 125 + F3 25 | 上游 PR | — | L1 ✔ L2 ✔ | 2 |
| T12 | HEIC 支持 | F2 中 heic 1 + F3 中改了扩展名的部分 | fork | T11 | L1 ✔ | 3 |
| T13 | JSON Schema 整数值浮点转整数（F1-a） | F1 中 27 | 上游 PR | — | L1 ✔ | 2 |
| T14 | xgrammar 不支持的约束：按 strict 分流（F1-b / F1-c） | F1 中 116 | 上游 PR（+ fork 临时方案） | 先确认 strict 取值 | L1 ✔ | 3 |
| T15 | OpenAI 标准错误体 + context_length 消息（W2-0 / E1） | E1 134 可被识别 | 上游 PR | — | L2 ✔ | 2 |
| T16 | tools strict schema 与带点函数名（E2） | 57 | cherry-pick #16482、#17163 + 上游小 PR | — | L2 ✔ | 1 / 4 |
| T17 | 请求体宽松反序列化（E3） | 49 | cherry-pick #18213 + 上游 PR | — | L2 ✔ | 3 |
| T18 | 历史 tool_calls arguments 容错（E4） | 22 | 上游 PR | — | L2 ✔ | 3 |
| T19 | E5 补公开消息 + DSV4.1 `reasoning_effort` 映射 | 46 中 44 | 上游 PR | T02 | L2 ✔ | 3 |
| T20 | 请求平面超限的固定文案改为公开 | 46 中 1 | 上游 PR（一行） | — | 代码推断 | 2 |
| T21 | `thinking` 参数容错（E6） | 1 | 上游 PR（可与 T18 合并） | — | L2 ✔ | 3 |
| T22 | frontend 连接开启 TCP keepalive | 配合链路问题 | 上游 PR | — | L2 ✔ | 3 |
| T23 | 向 SGLang / xgrammar 上游提 issue | F1 长期方案 | 外部 issue | T13、T14 | L1 ✔ | 3 |

**批次建议**
- **批次 1（本周）**：零代码或纯 cherry-pick，风险低，马上见效。T01、T02、T07，以及 T16 中 cherry-pick 的部分。
- **批次 2**：影响面大或纯增量的新代码。T09、T11、T13、T15、T20，以及可观测性 T04–T06。
- **批次 3**：兼容性放宽，需要开关。T12、T14、T17、T18、T19、T21、T22、T03、T23。
- **批次 4**：与上游现有设计冲突，必须先讨论。T08、T10，以及 T16 中“strict 显式降级为 false”的部分。

---

## 2. 标准修复流程（每个任务都按这个走）

### 2.1 任务状态

`待复现 → 已复现 → issue 已提 → 开发中 → PR 已提 → 上游已合 → fork 已回合 → staging 通过 → 线上验收 → 关闭`

- **配置 / fork 类**：跳过 issue 和 PR 两步，改为在 dynamo-recipes 或 fork 留变更记录。
- **cherry-pick 类**：issue 和 PR 两步改为“记录上游 PR 号”。
- **先讨论类**：在“issue 已提”停住，等维护者回复后再开发。

### 2.2 步骤

**① 建 task**

用卡片里的标题和验收条件。task 描述里贴卡片链接（`FIX_TASKS.md#tNN`）。

**② 复现（修复前）**

跑卡片里的复现命令，输出存到 `out/fix_repro/<TNN>/before.txt`。

开工前再确认一次上游是否刚刚修复：

```bash
cd /workspace/codes/dynamo
git remote add upstream https://github.com/ai-dynamo/dynamo.git 2>/dev/null; git fetch upstream
git log --oneline upstream/main -- <卡片里的文件> | head        # 看 11-16 之后有没有相关提交
gh search issues --repo ai-dynamo/dynamo "<关键报错文本>"       # 查重（本机未装 gh，需先安装并 gh auth login）
gh search prs    --repo ai-dynamo/dynamo "<关键报错文本>"
```

**③ 提 issue（上游）**

- 模板用 `bug_report`，字段为 Describe / Steps to Reproduce / Expected / Actual / Environment。功能放宽类用 `feature_request`。
- 卡片里的“issue 草稿”可以直接用：

```bash
gh issue create --repo ai-dynamo/dynamo --label bug --title "<卡片标题>" --body-file issue_TNN.md
```

- **脱敏规则**（上游是公开仓库）：
  - 只用卡片里的合成 payload 和本地复现输出；
  - 不放 trace_id、org ID、内部域名、IP、pod / 集群名、客户请求内容，也不提及客户；
  - SGLang 错误里带的 `data:` URI 前缀必须删掉（会带出约 58 字节客户媒体内容）。
- 分析文档要提交到 fork 时，用 `out/scripts/sync_to_dynamo.py` 同步，它会自动脱敏。

**④ 开发**

- 从 `upstream/main` 拉分支：`git switch -c fix/<slug> upstream/main`。
- **先写卡片里“修复前失败”的单测**，确认它失败，再改代码。
- 本机没有 cargo，需要在有 Rust 工具链的开发机或 dynamo dev 容器里跑：

```bash
cargo fmt --all && cargo clippy --workspace
cargo test -p dynamo-llm <测试名>          # 或 -p dynamo-runtime
pytest -m unit tests/ components/src/dynamo/sglang/tests/   # 改 Python 时
pre-commit run --all-files
```

**⑤ 本地端到端验证**

- 能在 main 上验证的，直接从源码装 main 再跑复现脚本。
- 否则先回合到 fork（见 ⑦），用 fork 构建再跑。
- 输出存为 `after.txt`，和 `before.txt` 一起放进 PR 的 Validation。

**⑥ 提 PR（上游）**

```bash
git commit -s -S -m "<卡片里的 PR 标题>"      # 提交身份与 fork 之前的提交保持一致
git push origin fix/<slug>
gh pr create --repo ai-dynamo/dynamo --head forestlee95:fix/<slug> --base main \
  --title "<卡片里的 PR 标题>" --body-file pr_TNN.md
```

- 每个 commit 都要 DCO 签名（`-s`）。要想 fork PR 自动跑 CI，commit 签名还需要显示为 Verified（`-S`，GPG 或 SSH 签名，公钥要先加到 GitHub）。否则需要维护者手动评论 `/ok to test <sha>`。
- PR 标题用 Conventional Commit 格式：`type(scope): summary`。
- PR 正文按仓库模板写四节：Overview / Details / Where should the reviewer start / Related Issues（写 `Closes #<issue>`）。另外按 AGENTS.md 要求，正文要包含 **Summary** 和 **Validation**（贴 before / after 和测试命令）。
- PR 标题和正文同样要遵守 ③ 的脱敏规则。

**⑦ 回合 fork**

```bash
git switch gmi/1.6.1-sglang-v0.5.23      # 或新开 gmi/1.6.1-fixes，集中放所有修复
git cherry-pick -x <上游合入的 sha>       # 上游还没合时，先 cherry-pick PR 分支上的 commit
```

- main 与 1.6.1 有结构差异，卡片里写了需要单独适配的地方。例如 T04 / T22：main 是自有的 accept 循环，1.6.1 用的是 `axum::serve`。
- 构建镜像，把 dynamo-recipes 中 dsv41 的镜像 tag 改为新镜像。

**⑧ staging（L4）**

- 部署到 dsv41 dev。
- 跑 `URL=... MODEL=deepseek-ai/DeepSeek-V4.1-Flash-1017 dynamo-recipes/smoke_dynamo_endpoint.sh` 做回归。
- 再跑卡片里的复现命令，确认得到修复后的期望输出。

**⑨ 线上验收**

- 上线满 24h 后，用 `out/scripts/` 按同样口径复跑分类：`collect.py` → `classify.py` → `report.py`。
- 对比对应子类的次数和形态变化，结果写回 task。
- 卡片里有专门的 OQL 时一并执行。

**⑩ 关闭条件（DoD）**

以下全部满足才能关闭：
- 上游 PR 已合（或维护者明确拒绝，且已记录“仅 fork 保留”的决定）；
- fork 已回合并上线；
- 线上验收达标；
- issue 已随 PR 自动关闭（或已手动关闭并链接到 PR）。

---

## 3. 任务卡

<a id="t01"></a>
### T01 开启 pre-commit 错误窥探（配置）

| 项 | 内容 |
|---|---|
| 解决 | F1–F5 共 302 次/天：从“HTTP 200 + 流内错误帧”变为直接返回 HTTP 400 |
| 代表性例子 | `113271f61ec960c549f070347c882543`（11-16 09:06:54）：frontend 发出 200 后 124ms 才出现错误；网关记为 `vendor_error_frame/invalid_request/400` |
| 改动 | dynamo-recipes dsv41 frontend 的 env 加 `DYN_HTTP_PRE_COMMIT_ERROR_PEEK_MS=1000`（位置：`kustomize/base/deploy.yaml` frontend `env:`，紧跟 `DYN_IGNORE_OPENAI_FE_UNSUPPORTED_FIELDS`） |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/f-class/l2 && bash run_l2.sh      # 第 1 段不设置，第 2 段设置 3000
# 手动：mocker 在请求省略 max_tokens 时会在流内返回 400
curl -sN -i localhost:18080/v1/chat/completions -H 'Content-Type: application/json' \
  -d '{"model":"Qwen/Qwen3-0.6B","stream":true,"messages":[{"role":"user","content":"hi"}]}'
```

**修复前 → 修复后（实跑）**

```
未设置:  [http 200]  data: {"error":{"message":"`max_output_tokens` (or `max_tokens`) is required by the mocker worker","type":"invalid_request_error","code":400}}
        frontend: INFO http response sent status=200 → WARN route request failed error.type="invalid_argument"
=3000:  HTTP/1.1 400 Bad Request  {"message":"`max_output_tokens` (or `max_tokens`) is required by the mocker worker","type":"Bad Request","code":400}
        frontend: ERROR http response sent status=400；正常请求仍是 200，首字节约 21ms
```

**窗口取值依据**

线上统计：frontend 发出 200 到出现第一条错误的间隔，F1 最长 33ms，F2 172ms，F3 159ms，F4 516ms，F5 170ms。**1000ms 能覆盖全部 302 次**。窗口从拿到 engine stream 开始计时，frontend 预处理的时间不算在内。

**风险 / 前置**
- 流式请求的响应头要等到首个 token 或窗口结束才发出，取两者中较早的那个；TTFT 本身不变。
- forward-worker 的响应头超时必须大于“窗口 + 预处理耗时”（预处理在 `113271f6` 中用了 1.35s）。
- 窗口期内连响应头都不会发出，所以 T03 的 SSE keep-alive 在这段时间里也不会发注释帧。

**验收**
- staging：用 T11 的 wav payload，直接返回 HTTP 400。
- 线上：24h 复跑后，F 类的 `vendor_error_frame/invalid_request/400` 基本消失，转为直接返回的 400。错误体仍是非 OpenAI 格式，这部分归 T15。

---

<a id="t02"></a>
### T02 E5 来源取证：临时打开 debug 日志一天（配置）

| 项 | 内容 |
|---|---|
| 目的 | 46 个 `{"message":"Invalid request"}` 里，44 个发生在 frontend 预处理阶段，但具体原因只在 DEBUG 级别记录。需要按原因聚合，给 T19 定优先级 |
| 代表性例子 | `c1aa0dc5c696b46bc2e4c449bcc2b924`（11-16 23:03:34，2MB，同一会话每轮都失败）；`73cdbe6dbe1dae694bbc2b0ff7fb1c0f`（18:06:20） |
| 改动 | 在 dsv41 frontend 上临时设置 `DYN_LOG="info,dynamo_llm::http::service::openai=debug,dynamo_llm::preprocessor=debug"`，24h 后撤掉。T06 上线后就不再需要 |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/e-class && ./harness/start_dsv41.sh       # DeepSeek-V4.1 tokenizer + deepseek_v41 parser，端口 18432
BASE=http://localhost:18432 MODEL=deepseek-ai/DeepSeek-V4.1-Flash-1017 ./repro_e.sh E5
```

开 debug 后可以看到原因，例如：

```
DEBUG ... Semantic request failure class=InvalidRequest reason=request.invalid_argument diagnostic=Some("DeepSeek V4.1 reasoning effort must be low, high, max, none, or an integer within 1–100")
```

**线上取数**

```bash
gmi-sre logs oql -q 'body: "Semantic request failure" AND timestamp:[<start_ms> TO <end_ms>]' -o out/e5_diag.json
```

按 `diagnostic=` 聚合。注意 diagnostic 里可能有客户文本，结果只留在 playground。

**风险**：debug 日志量会增加，preprocessor 模块的 debug 日志偏多。建议只开一个 frontend 副本，或者只开 `openai=debug`。

**交付**：一张“原因 → 次数”表，写回 T19。

---

<a id="t03"></a>
### T03 SSE keep-alive + 网关“无字节”超时（配置 + 网关）

| 项 | 内容 |
|---|---|
| 解决 | C1/C2/B2/B3：连接被撞断后，网关要等满 120s。打开后可缩短到约 20s |
| 代表性例子 | C1 `2a6fc286ab0fa11175ca8762b2d3a952`（11-16 04:36）：Dynamo 在 1,701ms 已认定断开，网关到 120,019ms 才超时，调用方 TTFT 为 124.7s |
| 改动 | Dynamo frontend 设置 `DYN_HTTP_SSE_KEEP_ALIVE_INTERVAL_MS=5000`（v1.6.1 已支持，`service_v2.rs:165-208`）；forward-worker 加“20s 内没收到任何字节就判定失败”的读超时 |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/obs && ./repro_disconnect.sh w52
```

**实跑结果**

```
默认：          72ms HTTP/1.1 200 → 12459ms 第一个 data:          （中间 0 字节）
=2000：         76ms HTTP/1.1 200 → 2077ms ':' → 4079ms ':' … → 12465ms data:
=2000 + T01 窗口 5000：  5086ms HTTP/1.1 200 → 7086ms ':' …     （窗口内连响应头都没有）
```

**前置 / 约束**
- 先在测试环境确认 forward-worker 的 SSE adapter 会忽略以 `:` 开头的行。
- 网关的“无字节”超时必须大于“T01 窗口 + frontend 预处理耗时 + SSE 间隔”。

**验收**：线上 C 类（`Backend stream timed out before returning any content`）的 `dur_ms` 分布从约 120s 降到约 20s；REPORT 2.2 中“网关比 Dynamo 晚”的 p50 从约 118s 降到约 20s。

---

<a id="t04"></a>
### T04 断开日志带 trace_id、request_id 和对端地址（P0-2 a+b，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | `Stream closed unexpectedly` 按 trace 查不到；断开的是哪条连接也无从得知（REPORT 只能按“同一毫秒”对齐）。这两点是链路根因取证的关键证据 |
| 代表性例子 | C1 `2a6fc286…` 04:36:04.143Z：这一行 `traceID=""`、`attributes={}`。`out/probe_disconnect.json` 中该窗口的 4 条全部为空 |
| main 是否仍存在 | 是。`disconnect.rs:306/321/347` 没有变化。main 的 `server.rs:80` 用 `Ok((stream, _))` 丢掉了对端地址 |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/obs && ./repro_disconnect.sh a     # 以及 b
# 手动：带 traceparent 的流式请求，客户端中途断开
python sse_client.py --url http://localhost:8000 --after 5 \
  --traceparent 00-2a6fc286ab0fa11175ca8762b2d3a952-1111111111111111-01 --request-id obs-a-rst
ss -tnp state established '( sport = :8000 )'        # 在 frontend 一侧查看对端端口
```

**修复前输出（实跑）**

```
INFO  metrics.rs:1711 request received          | trace_id=2a6fc286… request_id=332648df-…
INFO  route_span.rs:286 route request cancelled | trace_id=2a6fc286… request_id=332648df-…
WARN  disconnect.rs:346 Stream closed unexpectedly; issuing cancellation | (没有 trace_id / request_id)
ss: <frontend>:8000  <client>:50532   → frontend 日志中 0 行包含 50532
```

**不要用的临时办法**：设置 `DYN_LOG=…disconnect=trace` 后，这一行会有正确的 request_id，但 trace_id 是一个**新生成的根 trace**，比没有更误导（`./repro_disconnect.sh a-trace` 可复现）。

**修复要点**
- **(a) trace 上下文**：
  - `create_connection_monitor`（`disconnect.rs:293`）在 spawn 时挂父 span，例如 `info_span!(target: "request_span", "connection_monitor", request_id = …)`，再 `.instrument(span)`；
  - 删掉 `:320` 那个 trace 级别的 `#[instrument]`；
  - `:331` 和 `:346` 两条 `warn!` 都补字段：`request_id`、`elapsed_ms`、是否已开始输出、对端地址。
  - 调用点除了 openai.rs，还有 `anthropic.rs:438`、`sglang_generate.rs:340` 和 gRPC。
- **(b) 对端地址**：
  - **上游（main）**：`server.rs:77-96` 的 `accept()` 返回对端地址；在 `serve_connection` / `ConnectionService` 里给每个请求插入 `ConnectInfo<SocketAddr>`；`make_inference_request_span`（`logging.rs`）记录 `client.address` / `client.port`。
  - **fork（1.6.1）**：`service_v2.rs:1073` 改为 `into_make_service_with_connect_info::<SocketAddr>()`。HTTPS 分支 `:993-995` 也要改。
- **线上前提**：先确认 frontend Service 的 `externalTrafficPolicy`。设为 Cluster 时，地址会被 SNAT 成节点 IP，看不到 NLB 保留下来的客户端 ip:port。

**修复前失败的单测**
- `disconnect.rs` 的 tests：用 `CaptureLayer` 捕获事件（仿照 `tcp/client.rs:812` 的 `upstream_cancellation_event_is_parented_to_worker_request_span`）。在 `http-request` span 内调用 `create_connection_monitor`，drop 已 arm 的 handle，断言事件的父 span 链里有 `http-request`，并且带 `request_id`。要用 `#[tokio::test]`（current_thread）。
- main 的 `server.rs`：在 `accepted_connections_turn_nagle_off` 旁加用例，断言 handler 能从扩展里取到 `ConnectInfo`，且等于客户端的 `local_addr()`。

**修复后期望**：`./repro_disconnect.sh a` 中 WARN 那一行带 `trace_id=2a6fc286… request_id=… elapsed_ms=… client.address=<client ip> client.port=…`。

**线上验收**
- `gmi-sre logs oql -q 'body: "Stream closed unexpectedly" AND timestamp:[…]'`，所有行的 traceID 都不为空。
- 对任一 C 类 trace 执行 `gmi-sre logs for-trace`，结果中能看到这一行。

**issue 草稿**
- 标题：`[BUG]: frontend "Stream closed unexpectedly" warning has no trace context or peer address`
- 正文要点：带 traceparent 的请求，在客户端中途 RST 时：
  - 同一请求的其他日志行都带 trace_id，只有这一行没有；
  - 打开 trace 级别后，这一行会得到一个新的根 trace；
  - 日志里没有对端地址，无法和客户端的连接对应。
  - 附上本地复现命令和输出。

**PR**
- `fix(frontend): attach request span and peer address to disconnect logs`
- 可以拆成 (a)(b) 两个 PR；(b) 与 T22 改的是同一处 `server.rs`。

---

<a id="t05"></a>
### T05 `route request failed` 记录（截断、脱敏后的）错误消息（P0-2 c，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | 后端拒绝的真实原因在 frontend 日志里完全看不到。例如 F5 的原因，只出现在 decode pod 不带 trace_id 的 traceback 里 |
| 代表性例子 | `113271f6…`：`{"body":"route request failed","attributes":{"error.type":"backend_invalid_argument","request.outcome":"rejected"}}`。真实原因 `ValueError: Requested token count exceeds … 289713 input + 780979 completion` 只能按时间窗在 decode pod 日志里找到 |
| main 是否仍存在 | 是（`route_span.rs:301` 的签名没变） |

**复现（L2，已实跑）**：`out/fix_repro/f-class/l2/run_l2.sh` 第 1 段。

**修复前输出**

```
WARN … route request failed request.outcome="rejected" error.type="invalid_argument" … otel.status_description="invalid_argument"
```

真实原因 `` `max_output_tokens` (or `max_tokens`) is required … `` 在 frontend 日志中没有出现。

**修复要点**
- `route_span.rs:297` 的 `finish_error` 增加消息参数。调用方是 `poll_next`（:358 拿得到 `error`）和 `record_route_error`（:140-150）。
- 消息截断到约 512 字符，并对 `data:` URI 脱敏：SGLang 的消息会带 data URI 的前 100 个字符，约 58 字节客户媒体内容。可以复用 `preprocessor.rs:3912-3918` 的 `url_for_log` 逻辑。
- 只写进日志和 span 的 `otel.status_description`，不进客户端响应，也不作为 metric label（`lib/runtime/src/error.rs:41-45` 的约定）。
- `error.type == cancelled` 时不加这个字段。

**修复前失败的单测**：在 route_span tests 中仿照 `route_span_covers_attempt_lifecycle_and_retry_metadata`，新增 `route_failure_records_bounded_error_message`。断言：事件含 `error.message`；长度 ≤ 上限；`data:image/png;base64,AAAA…` 被替换为 `<redacted>`。

**修复后期望**

```
WARN route request failed … error.type="backend_invalid_argument" error.message="ValueError: Requested token count exceeds the model's maximum context length …"
```

**PR**：`fix(runtime): include bounded, redacted backend error message in route failure logs`

---

<a id="t06"></a>
### T06 校验失败的原因记 WARN（P0-2 d，范围比 FIX_PLAN 大，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | E1–E6 的原因目前都不在 frontend 日志里：E1/E5 只在 DEBUG 记录；E2/E4/E6 完全不记；E3 连 `request completed` 都没有，只有一行 `http response sent status=400` |
| 代表性例子 | E5 `dcdaa2125bbb933b1e52ed6558560180`；E3 `4d6f797d3ef4f81f20d15638ef7ef92b`（Dynamo 侧只有 `status=400 latency_ms=0`） |
| main 是否仍存在 | 是（`openai.rs:914-930` 相同） |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/e-class && DYN_LOG=info ./harness/start_qwen.sh
BASE=http://localhost:18431 ./repro_e.sh
```

frontend 日志里只有 `request completed … error_type=validation error_detail=invalid request parameters`，没有原因。

**修复要点（三处）**
1. `from_semantic_error_with_recording`（`openai.rs:785-799`）：InvalidRequest 类从 `debug!` 改为 `warn!`，带 class、reason 和截断后的 diagnostic。diagnostic 可能含用户文本，需要截断；是否需要开关，在 PR 里和上游讨论。
2. `from_http_error` 的 `ForwardClientError` 分支（:925-935）：对 4xx 记 `warn!(status, message = 截断后的 message)`。这些 message 本来就已经返回给客户端，记日志没有额外泄露。
3. `parse_json_request` 失败时（:2626 / :2629）补一行 `warn!(endpoint, error)`。

**修复前失败的单测**：把日志级别选择抽成 `fn semantic_failure_log_level(class) -> Level`，新增 `invalid_request_semantic_failures_log_at_warn`。修复前返回 DEBUG，测试失败。

**修复后期望**：默认 `DYN_LOG=info` 时，`repro_e.sh` 的每个 case 在 frontend 日志里都有一行 WARN，带具体原因。T02 因此不再需要。

**PR**：`fix(frontend): log request validation failure reasons at warn`

---

<a id="t07"></a>
### T07 SGLang prefill abort 上报（P0-2 e，cherry-pick）

| 项 | 内容 |
|---|---|
| 解决 | prefill 的 bootstrap 超时被 Dynamo handler 当作正常结束：frontend 看不到 `Prefill background task failed`，prefill 的 route span 记为 success |
| 代表性例子 | F5 `113271f6…`：SGLang 自己打了 `Prefill bootstrap failed … timed out after 300.0s in KVPoll.Bootstrapping`，frontend 侧没有任何对应日志 |
| 上游状态 | **main 已修**：58e83f6d2 `fix(router): cancel peer work on disaggregated request failures (#16766)` |

**复现**
- L1（代码推断，需要能 import sglang 的环境，例如 SGLang runtime 镜像，不需要 GPU）：把 main 的 `test_prefill_propagates_backend_abort`、`test_prefill_bootstrap_abort_after_router_cancellation`（`components/src/dynamo/sglang/tests/test_sglang_prefill_handler.py`）拷到 1.6.1 上运行。在 1.6.1 上 `pytest.raises(HttpError)` 会失败。
- L3：按 T10 的 L3 步骤，在 prefill 上设 `SGLANG_DISAGGREGATION_BOOTSTRAP_TIMEOUT=30`。30s 后 frontend 只有 `route request completed request.outcome="success"`。

**改动**
- 只 cherry-pick 58e83f6d2 中 `prefill_handler.py` 及其测试：`_consume_results` 对 dict 类型的 finish_reason 调用 `_raise_if_sglang_error`。
- 这个 commit 的 Rust 部分（handoff.rs / admission.rs，按 CapacityExhausted 取消 prefill）放到 T10 一并评估，不要在这里混进去。

**修复后期望**：frontend 出现 `WARN Prefill background task failed error=… Prefill bootstrap failed …`，带 trace_id。

**PR**：不提上游。fork 内 commit 信息写 `(cherry picked from commit 58e83f6d2)`。

---

<a id="t08"></a>
### T08 区分取消来源，降级 `Failed to publish complete final for stream`（P0-2 f，先讨论）

| 项 | 内容 |
|---|---|
| 解决 | 客户端只要断开（包括正常的 FIN 取消），SGLang decode 就会打一条 ERROR，并让 `dynamo_component_errors_total{error_type="publish_final"}` 加 1。分析时容易被误读为 worker 故障 |
| 代表性例子 | C1 `2a6fc286…`：decode 在 1,701ms 收到 `request cancellation received signal=kill source=upstream`，1,710ms 报 `ERROR Failed to publish complete final for stream` |
| 与上游的冲突 | 上游 PR #12374 有意让 kill 保持 ERROR，并有测试守着：`test_publish_final_with_killed_context_is_still_an_error`、`test_publish_final_after_connection_read_error_is_still_an_error`（`push_handler.rs:1456/1468`）。原因是在 context 上，`ControlMessage::Kill` 和 TCP 读错误无法区分。**照 FIX_PLAN 的写法直接降级，会被这两个测试拦下** |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/obs && ./repro_disconnect.sh f
```

mocker 取消后还会再发一帧，复现不出来；要用 `stop_worker.py`，它模拟 SGLang decode 取消后不再发帧。

**修复前输出（实跑）**

```
INFO  client.rs:37 request cancellation received | trace_id=2a6fc286… request_id=bfe3ab55-…
ERROR push_handler.rs:381 Failed to publish complete final for stream bfe3ab55-…   ← RST、FIN、响应头后 RST 三种断开方式都会出现
```

**修复要点**
- 在 `tcp/client.rs:638` 处理 `ControlMessage::Kill` 时，给 context 记一个“上游主动取消”的来源标记，例如 `cancel_source()`。
- `push_handler.rs:369` 的降级条件改为 `is_stopped() && (!is_killed() || cancel_source()==Upstream)`。
- 读错误（`client.rs:663-668`）仍记 ERROR。

**修复前失败的单测**：在 `push_handler.rs` 的 `run_teardown_race` 组新增 `Teardown::UpstreamKill`，断言 `publish_final == 0`。现有的 `Kill`（无来源标记）和 `ConnectionReadError` 两个用例保持断言为 1。

**修复后期望**：`./repro_disconnect.sh f` 不再出现 ERROR。线上 `Failed to publish complete final` 只剩和 `tcp stream read error` 同时出现的那些。

**issue 草稿（先发 issue，带上设计问题）**
- 标题：`[BUG]: upstream client cancellation is logged as ERROR "Failed to publish complete final for stream" on SGLang decode workers`
- 正文：说明正常的 FIN 取消也会触发；引用 #12374 的设计理由；提出“给 Kill 标记来源”的方案，请维护者确认。

**PR**：`fix(runtime): distinguish upstream kill from connection errors when publishing final response`

---

<a id="t09"></a>
### T09 含图请求的长度下界检查（F4 + F5，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | F4 3 次 + F5 5 次 / 天。F5 根因已用线上 decode 日志确认，5 条全部对上，这项能把它们全部拦在 frontend |
| 代表性例子 | F4 `6a52b1c27c3c6a2526c79cdeebe979f9`（11-16 18:29:40）：`The input (1234754 tokens) is longer than the model's context length (1048576 tokens)`。F5 `113271f6…`：decode `Requested token count exceeds … 289713 input + 780979 completion = 1070692` |
| 根因 | `exact_prompt_len`（`preprocessor.rs:1849-1874`）在有图片、但拿不到 MM 展开长度时返回 None，于是 `validate_preprocessed_token_budget`（:1930-1955）整段跳过。DeepSeek-V4.1 不在 MM routing 支持列表里（`mm_routing/image.rs:158-164`），永远拿不到展开长度 |
| main 是否仍存在 | 是（main:2143 / :2224） |

**复现**
- **L1' Rust 单测（推荐；本机没有 cargo，未实跑）**：在 `preprocessor.rs` 的 `test_preprocessed_completion_budget_validation_and_deferral`（:13559）之后新增 `test_multimodal_lower_bound_budget`。复用 `preprocessed_budget_request`（:13461，`token_ids=[1,2,3]`）和 `token_budget`（:13415），设 `multi_modal_data={"image_url":[Url("data:image/png;base64,AA==")]}`、`mm_routing_info=None`。断言：
  1. `max_tokens=8`，`token_budget(10,true,true)`：应当报错。**修复前返回 `Ok(None)`，测试失败**；
  2. `token_ids` 长度为 10，`max_tokens=None`，`token_budget(10,true,false)`：应当报 `prompt_overflow_error`；
  3. `max_tokens=6`，`token_budget(10,true,true)`：必须 `Ok(None)`。没超时不能误拒，也不能给省略的 max_tokens 设默认值；
  4. `token_budget(10,false,false)`：任何情况都 Ok。
- **L2 不能复现**：mocker 不发布 `token_budget`，frontend 对 mocker 完全不做长度预检。
- **L3（单卡）**：

```bash
python3 -m dynamo.frontend --http-port 8000 &
python3 -m dynamo.sglang --model-path OpenGVLab/InternVL3-1B --served-model-name m \
  --enable-multimodal --trust-remote-code --context-length 4096 --page-size 16 &
cd out/fix_repro/f-class && ../venv/bin/python -I make_payloads.py --model m --ctx 4096
curl -sN -w '\n[http %{http_code}]\n' localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d @payloads/f4_control_text_only.json
curl -sN -w '\n[http %{http_code}]\n' localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d @payloads/f4_image_plus_overlong_text.json
curl -sN -w '\n[http %{http_code}]\n' localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d @payloads/f5_image_total_overflow.json
```

模型不能用 Qwen2.5-VL / Qwen3-VL，它们在支持列表里，frontend 能正常预检。

**修复前（代码推断）**
- 纯文本对照组：直接返回 400，`details.type=context_length`。
- 含图：200 + 流内 `The input (N tokens) is longer than the model's context length (4096 tokens).`。

**修复要点**
- 含图、但没有展开长度时，仍用 `token_ids.len()` 做下界检查。图片占位符展开后只会变长，所以下界超限就一定超限。
- `reject_total_overflow` 为真时，同样检查 `token_ids.len() + max_tokens`。
- 返回 None 的语义不变，不给省略的 max_tokens 补默认值。

**修复后期望**：含图的 F4 / F5 payload 在提交 200 之前就返回 400，`details.type=context_length`。线上 F4 / F5 降到 0 或接近 0。

**例外**：`a15b98fd…` 只超出 26,702 token，只有图片少于 27 张时下界检查才能拦住。其余的要等精确计数（为 DeepSeek-V4.1 实现 `ImageRoutingBackend`，单独开 task），或者靠 T01 返回 400。

**issue 草稿**
- 标题：`[BUG]: multimodal requests skip the frontend context-length check when the model has no MM prompt-layout support`
- 正文：用 InternVL3-1B 的 L3 步骤。说明在 disagg 模式下，decode 会因为“输入 + max_tokens”超限而拒绝，而 prefill 用的是 max_tokens=1，会一直空等到 bootstrap 超时（引用 T10）。

**PR**：`fix(preprocessor): lower-bound context check for multimodal prompts without exact MM length`

---

<a id="t10"></a>
### T10 decode 以 InvalidArgument 拒绝后，取消配对的 prefill（F5，先讨论）

| 项 | 内容 |
|---|---|
| 解决 | decode 在 tokenizer 阶段就拒绝了请求，prefill 却一直等到 300s bootstrap 超时，期间占着 prefill 资源，frontend 的 http-request span 也持续约 301s |
| 代表性例子 | F5 `113271f6…`：decode 在约 120ms 拒绝，prefill 在 300s 后报 `Prefill bootstrap failed … KVPoll.Bootstrapping` |
| 与上游的冲突 | main（#16766）只在 decode 返回 `CapacityExhausted` 时取消 prefill（`handoff.rs:282-297`），并有测试 `other_invalid_argument_decode_errors_preserve_prefill_transfer`（:886-927）**明确要求** `Backend(InvalidArgument)` 时**不**取消。main 的 `forward_decode` 已经拿到了 `prefill_context` |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/f-class/l2 && bash run_l2.sh       # 第 3 段：prefill / decode 两个 mocker
```

触发方式是省略 `max_tokens`：decode 在流内拒绝，prefill 固定用 max_tokens=1，所以正常接受。

**修复前输出（实跑）**

```
03:04:48.066 WARN route request failed request.outcome="rejected" error.type="invalid_argument" worker_id=<decode>
03:05:18.130 (prefill) request completed           ← 30s 后才结束
03:05:18.131 WARN Prefill background task failed error=… handoff participant rendezvous timed out
```

mocker 的超时是 30s，SGLang 是 300s。

**修复要点**
- 基于 main：把 `Backend(InvalidArgument)` 加到取消 prefill 的条件里，并用 worker 已发布的 `disagg_prefill_cancel_anytime_v1` 能力作开关。理由是：SGLang decode 在 tokenizer 阶段就报 InvalidArgument 时，还没有连接 bootstrap，不存在需要保留的 KV 传输。
- 客户端断开时仍然不取消 prefill，保持 `disconnected_client_does_not_drop_prefill_transfer` 不变。
- fork（1.6.1）没有 `prefill_context`，需要连同 58e83f6d2 的 Rust 部分一起回合，或者照 main 的签名补上。

**修复前失败的单测**
- main：修改 `other_invalid_argument_decode_errors_preserve_prefill_transfer`，把 `Backend(InvalidArgument)` 移到“取消”组（仅在开关打开时），复用 `assert_stream_rejection_cancels_prefill` 和 `assert_dispatch_rejection_cancels_prefill`。
- 1.6.1：新增 `decode_stream_invalid_argument_cancels_prefill`，断言 `prefill_context.is_stopped()`，且清理在 1s 内完成。

**修复后期望**：L2 中不再出现 `rendezvous timed out`，prefill 的 `request completed` 紧跟在 decode 之后。L3 / 线上不再出现 `Prefill bootstrap failed … 300.0s`。

**缓解（配置，可以先做）**：适当调低 prefill 的 `SGLANG_DISAGGREGATION_BOOTSTRAP_TIMEOUT`，例如 60。风险是 decode 排队较长时，会误杀正常但较慢的 bootstrap。T09 上线后，F5 已经能在 frontend 拦住，所以这项的优先级降低。

**issue 草稿**
- 标题：`[DISCUSSION]: cancel paired prefill when SGLang decode rejects with InvalidArgument before bootstrap`
- 正文：附 mocker 的复现；引用 `other_invalid_argument_decode_errors_preserve_prefill_transfer` 的设计；提出按能力开关启用。

**PR**：`fix(router): cancel paired prefill on pre-bootstrap decode InvalidArgument when the worker supports it`

---

<a id="t11"></a>
### T11 data URI 媒体格式嗅探（F2 / F3，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | F2 125 次（audio/wav 92、pdf 32、heic 1，涉及 49 个 org）+ F3 25 次。这些请求现在都是“200 + 流内错误”，改成在流开始前直接返回明确的 400 |
| 代表性例子 | wav `0f27cd60d37b79270627dfbc0da744e9`（08:27:52）；pdf `2d9f5a52379d7ce54260cfbcc718d257`；heic `569892ee9b4d2518fbe2709c3583b71b`；F3 png `4f0cd473642b80fb77f64577add3a9f0` |
| main 是否仍存在 | 是（main 的 `preprocessor.rs` 只检查 data URL 大小） |

**复现**
- L1（已实跑）：

```bash
cd out/fix_repro/f-class && ../venv/bin/python -I make_media.py && ../venv/bin/python -I repro_f2f3.py
```

- L2（已实跑，只能看到 frontend 侧；mocker 不解码媒体，请求以 200 结束）。最小 payload：

```bash
curl -sN localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","stream":true,"max_tokens":16,"messages":[{"role":"user","content":[{"type":"text","text":"Describe the attachment."},{"type":"image_url","image_url":{"url":"data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQAAAAA="}}]}]}'
# F3：SVG 标成 png
curl -sN localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","stream":true,"max_tokens":16,"messages":[{"role":"user","content":[{"type":"text","text":"Describe the attachment."},{"type":"image_url","image_url":{"url":"data:image/png;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI4IiBoZWlnaHQ9IjgiPjxyZWN0IHdpZHRoPSI4IiBoZWlnaHQ9IjgiLz48L3N2Zz4="}}]}]}'
```

- L3：任意 SGLang VLM，单卡，用上面两条 curl。

**修复前输出（实跑）**

```
L1 worker: InvalidRequest: ValueError: An exception occurred while loading IMAGE data at index 0: Error while loading data data:audio/wav;base64,UklGR…: Could not decode image: cannot identify image file <_io.BytesIO …>
L2 frontend: WARN mm-routing: failed to fetch image dims; MM routing entry skipped url=data:audio/wav;base64,<redacted> error=fetch_image_dims failed: The image format could not be determined
```

`repro_f2f3.py` 还打印出嗅探结果：wav → `audio/wav`，svg → `image/svg+xml`，html → `text/html`，heic → `image/heic`。

**修复要点**
- 位置：`preprocessor.rs` 中 data URL 大小检查之后（:3663-3681）。
- 只解码 base64 的前几十个字符，用 `image::guess_format` 识别格式，再显式识别 HEIC（`ftyp` + heic / heix / mif1 / msf1）、`%PDF-`、RIFF/WAVE、ID3、OggS、fLaC。
- 只放行 JPEG / PNG / GIF / WEBP / BMP / TIFF。其他格式返回固定的公开 400，例如 `image_url only supports images; received audio/wav`，消息里**不带** base64 内容。
- 按魔数判断，不按声明的 MIME 判断。
- 这项检查必须始终执行：不依赖 `mm-routing` feature，也不要在请求带 user uuid 时跳过（:3872）。
- HTTP URL 不下载就检查不了，靠 T01 兜底。

**修复前失败的单测**：在 `preprocessor.rs` tests 中新增 `data_uri_media_sniff_rejects_non_image`，参数化以下输入：wav、`%PDF-`、HEIC ftyp、SVG 标成 png、HTML 标成 jpeg、真 PNG、“声明 png 实际 JPEG”、“声明 png 实际 GIF”。断言前 5 个返回 InvalidArgument 且消息里没有 base64；后 3 个通过。

**修复后期望**
- L2 / L3：wav / svg 的请求在提交 200 之前返回 HTTP 400，消息清晰，不再派发到 worker。
- 线上：F2 / F3 变为直接返回的 400。

**issue 草稿**
- 标题：`[BUG]: non-image data URIs in image_url are accepted by the frontend and fail inside the worker after HTTP 200`
- 正文：附 wav / svg 的合成 payload，以及 L1 中 worker 的报错。指出 SGLang 的消息会带出 data URI 的前 100 个字符。

**PR**：`feat(preprocessor): sniff data-URI media and reject non-image payloads before streaming`

---

<a id="t12"></a>
### T12 HEIC 支持（fork）

| 项 | 内容 |
|---|---|
| 解决 | F2 中的 heic 1 次，以及 F3 中改了扩展名的 HEIC（数量待 T11 上线后，从嗅探日志里统计） |
| 代表性例子 | `569892ee9b4d2518fbe2709c3583b71b`（13:18:39，2.2MB） |
| 复现（L1，已实跑） | `repro_f2f3.py` 中的 `f2_heic` 和 `f3_heic_as_png`：注册前 `cannot identify image file`，调用 `register_heif_opener()` 后 `OK (decoded)` |
| 改动 | SGLang runtime 镜像加装 `pillow-heif`。在 Dynamo SGLang worker 创建 `sgl.Engine` 之前（`components/src/dynamo/sglang/init_llm.py` 或 `main.py`）调用 `register_heif_opener()`，prefill 和 decode 都要做。完成后 T11 的白名单加上 HEIC |
| 风险 | HEVC 专利许可问题；libheif 的攻击面。上线前需要确认许可 |
| 验收 | L3 / staging：用 `out/fix_repro/f-class/media/payload_f2_heic.json`，返回 200 并正常描述图片 |

---

<a id="t13"></a>
### T13 JSON Schema 中整数值的浮点转为整数（F1-a，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | F1 中 27 次 / 天（`maxLength must be an integer`） |
| 代表性例子 | `5608000571c608d75ff9e176465af70b`（11-16 12:28:09） |
| 根因 | xgrammar 要求 minLength / maxLength 这类字段是整数，`100.0` 会报错。Dynamo 从 serde → pythonize → `json.dumps` 一路保留浮点形式。**更正 FIX_PLAN**：frontend 对 `response_format` 不做任何 schema 校验，`strict_schema.rs` 只管 `tools[].function` 且 `strict:true` 的情况 |
| main 是否仍存在 | 是 |

**复现（L1，已实跑）**

```bash
cd out/fix_repro/f-class && ../venv/bin/python -I repro_f1.py
```

e2e payload（L3，任意文本模型，单卡）：

```bash
curl -sN localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","stream":true,"max_tokens":64,"messages":[{"role":"user","content":"Return a JSON object."}],"response_format":{"type":"json_schema","json_schema":{"name":"f1_5","strict":false,"schema":{"type":"object","properties":{"title":{"type":"string","maxLength":100.0}},"required":["title"]}}}}'
```

**修复前输出（实跑）**

```
== F1-5 maxLength: 100.0
  SGLang dispatch_json : ABORT  Failed to compile json grammar: … json_schema_converter.cc:4175: maxLength must be an integer
  after F1-a int-normalize: OK     grammar compiled
```

**修复要点**
- 在 `get_guided_json`（`chat_completions.rs:463-488`）或写入 `guided_decoding.json` 的地方（`preprocessor.rs` 约 3193），对以下字段把整数值的浮点转为整数：min/maxLength、min/maxItems、min/maxProperties、min/maxContains。值为 null 时去掉该字段。
- tool 参数 schema 同样处理。
- `100.5` 这类非整数保持原样，交给后端报错。

**修复前失败的单测**：`chat_completions.rs` 新增 `guided_json_normalizes_integral_float_bounds`，断言 `maxLength` 为 `Value::Number(100)`（`is_u64()`）、序列化结果里不含 `100.0`，`100.5` 保持不变。

**修复后期望**：L3 返回 200 并正常生成。线上 `maxLength must be an integer` 降到 0。

**PR**：`fix(llm): normalize integral float bounds in JSON schemas for guided decoding`。同时向 xgrammar 提 issue，见 T23。

---

<a id="t14"></a>
### T14 xgrammar 不支持的约束：按 strict 分流（F1-b / F1-c，上游 PR + fork 临时方案）

| 项 | 内容 |
|---|---|
| 解决 | F1 中 116 次 / 天：format+pattern 44、uniqueItems 32、if/then 21、pattern+长度 19。全部来自同一个 org |
| 代表性例子 | format+pattern `378cf848844beaef98d508dd34b7f097`（14:47）；uniqueItems `241680d87b25e1c345d812c25b3468a7`；pattern+长度 `9027ff064e333b2f671427da2cec83ae`；if/then `07055b051a6d56a95f5af467c229d2cc` |
| 关键事实（已实跑） | 这 4 种 schema **xgrammar 0.2.7 本身都能编译**，只是会静默丢掉这些约束。拒绝它们的只有 SGLang 的预校验 `validate_xgrammar_json_schema` |
| 前置 | 先确认这个 org 的请求里 `json_schema.strict` 取什么值：`gmi-sre logs for-trace --include-request-data` 只看这一个字段（日志含客户内容，注意权限）。strict 不为 true 走 F1-b，为 true 走 F1-c |

**复现（L1，已实跑）**：`../venv/bin/python -I repro_f1.py`。5 条 payload 在 `out/fix_repro/f-class/payloads/f1_{1..5}.json`。

**修复前输出（实跑，与线上逐字一致）**

```
== F1-1 format+pattern (anyOf)  SGLang: ABORT … correctsVersionId/anyOf/0 is not supported by xgrammar: constraints format, pattern cannot be enforced together   xgrammar alone: OK
== F1-2 uniqueItems             SGLang: ABORT … keyword(s) uniqueItems would be ignored                                                                        xgrammar alone: OK
== F1-3 pattern+min/maxLength   SGLang: ABORT … constraints pattern, minLength/maxLength cannot be enforced together                                            xgrammar alone: OK
== F1-4 if/then (allOf)         SGLang: ABORT … keyword(s) if, then would be ignored                                                                           xgrammar alone: OK
```

**修复要点**
- **F1-b**（strict 不为 true；opt-in，加上能力开关）：
  - 新模块放在 `protocols/openai/strict_schema.rs` 旁，在 `preprocessor.rs` 约 7659-7663 调用；
  - 去掉 11 个不支持的关键字；format 与 pattern 冲突时二选一；
  - 记录被移除约束的 JSON pointer 并计数；
  - 只修改给 grammar 用的副本，chat template 仍然看到原始 schema。
- **F1-c**（strict 为 true）：用 Rust 移植 `validate_xgrammar_json_schema`，错误文本与 SGLang 原文逐字一致，在提交 200 之前返回 400。用上面 5 个 schema 做 golden case。
- **F1-d**（只在 fork 做的临时方案）：通过 SGLang 插件入口，把预校验改为只告警。xgrammar 本身能编译，所以技术上可行；代价是约束被静默丢掉。

**修复前失败的单测**：新模块的表驱动测试。F1-b 断言输出 schema 中不再有这些关键字，并且移除清单正确；F1-c 断言错误文本与 `out_f1.txt` 一致。

**修复后期望**
- strict≠true：返回 200 并正常生成，frontend 记一行 WARN，列出被移除的约束。
- strict=true：在提交 200 之前返回 400，消息与 SGLang 原文一致。

**PR**：`feat(llm): relax or prevalidate response_format schemas for xgrammar`。同时向 SGLang 提议给预校验加策略开关，见 T23。

---

<a id="t15"></a>
### T15 OpenAI 标准错误体 + context_length 消息（W2-0 / E1，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | E1 134 次 / 天能被调用方识别为 `context_length_exceeded`，次数本身不会下降。同时所有 400 都会带上 OpenAI 标准的 `error` 对象 |
| 代表性例子 | `103d176d989be5684cc803aabfb9af3a`（11-16 03:47:32，body 3.18MB）：`{"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1052243}}` |
| main 是否仍存在 | 是（`ErrorMessage` 只多了 `utoipa::ToSchema`；`from_semantic_error_with_recording` 逐行相同） |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/e-class && ./harness/start_qwen.sh        # 补发 token_budget=1024；纯 mocker 不会触发 E1
BASE=http://localhost:18431 ./repro_e.sh E1
# 手动：
LONG=$(printf 'hello %.0s' $(seq 1 1500))
curl -s localhost:18431/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"Qwen/Qwen3-0.6B","max_tokens":16,"messages":[{"role":"user","content":"'"$LONG"'"}]}'
curl -s localhost:18431/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"Qwen/Qwen3-0.6B","max_tokens":4096,"messages":[{"role":"user","content":"hi"}]}'
```

**修复前输出（实跑）**

```
E1a 400 {"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1024,"actual":1509}}
E1b 400 {"message":"Invalid request","type":"Bad Request","code":400,"details":{"type":"context_length","limit":1024,"actual":4105}}
```

**修复要点**
- `ErrorMessage`（`openai.rs:240-250`）改为手写 Serialize：保留顶层的 `message/type/code/details`，**另外加**嵌套的 `error:{message,type,param,code}`。
  - `type` 复用 `disconnect.rs:393-411` 中已有的状态码映射；
  - E1 的 `code` 为 `context_length_exceeded`，`param` 为 `messages`。
- E1 的 message 在 HTTP 渲染层用 limit / actual 拼出来。`DynamoError` 只有一个 public 槽，`public_message` 和 `ContextLength` details 不能同时设置。文案可以照抄 diagnostic 里的现成格式。
- `"Invalid request"` 这段文字来自 `lib/llm/src/http/service/error.rs:61`。

**修复前失败的单测**
- `openai.rs`：仿照 `frontend_invalid_argument_exposes_client_safe_details`（:7656），新增 `context_length_error_renders_openai_message_and_code`。
- 新增 `error_body_has_openai_error_object`。
- `lib/llm/tests/frontend_protocol_validation_http.rs`：加辅助函数 `assert_openai_error_object`。

**修复后期望**

```json
{"message":"This model's maximum context length is 1048576 tokens. However, your messages resulted in 1052243 tokens. Please reduce the length of the messages.",
 "type":"Bad Request","code":400,"details":{"type":"context_length","limit":1048576,"actual":1052243},
 "error":{"message":"This model's maximum context length is …","type":"invalid_request_error","param":"messages","code":"context_length_exceeded"}}
```

**PR**：`fix(frontend): OpenAI-compatible error object and context_length_exceeded message`

---

<a id="t16"></a>
### T16 tools strict schema 与带点的函数名（E2，cherry-pick + 上游小 PR）

| 项 | 内容 |
|---|---|
| 解决 | 57 次 / 天：required 缺属性 39，uniqueItems、additionalProperties、带点函数名合计 18 |
| 代表性例子 | required `5ebff0bcb88508894c383a121a2a27b3`（14:30:58）；uniqueItems `e817afc6478ebe2422df834d6b7c8e99`；additionalProperties `5ff5fd186bc86403772d34f62145f7ce`；带点函数名 `852fcbf1b72e522ed22a45f410c814d7` |
| 上游状态 | 带点函数名：**main 已修**（3032e8a70 #16482）。strict 校验：main 增加了 opt-in 开关 `DYN_DISABLE_OPENAI_STRICT_SCHEMA_VALIDATION`（a0de541ea #17163），但开关打开时 **`strict:true` 仍原样下传**。dsv41 开着 structural tag，这些 schema 下传后可能变成 F1 那样的流内错误 |

**复现（L2，已实跑，与模型无关）**：`BASE=… ./repro_e.sh E2`。最小 payload：

```bash
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","max_tokens":8,"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"flag_issue","strict":true,"parameters":{"type":"object","properties":{"summary":{"type":"string"}},"additionalProperties":false}}}]}'
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","max_tokens":8,"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"files.get_file","parameters":{"type":"object","properties":{}}}}]}'
```

**修复前输出（实跑，与线上逐字一致）**

```
400 {"message":"Validation: Invalid schema for function 'flag_issue': In context=#/required, must include property 'summary'","type":"Bad Request","code":400}
400 {"message":"Validation: Function at index 0 has an invalid name: \"files.get_file\". Only a-z, A-Z, 0-9, underscores, and dashes are allowed.","type":"Bad Request","code":400}
```

**步骤**
1. **批次 1（fork）**：
   - cherry-pick #16482 和 #17163，连同它们的测试；
   - dsv41 frontend 设置 `DYN_DISABLE_OPENAI_STRICT_SCHEMA_VALIDATION=1`；
   - 在 L3 上确认：开关打开后，上面的 strict payload 不会变成 xgrammar / structural tag 的流内错误。
2. **批次 4（先讨论）**：
   - 在 #17163 的基础上，开关打开且 strict 校验失败时，把该 tool 的 `strict` **显式**设为 `Some(false)`。必须是显式的，因为 structural tag 会把未设置当作 true。
   - Responses API（`responses/mod.rs` 约 842-845）同样处理。

**修复前失败的单测**：`frontend_protocol_validation_http.rs` 中 main 已有 `.env(DYN_DISABLE_OPENAI_STRICT_SCHEMA_VALIDATION,"1")` 的测试，在其上加断言“下游收到的 tool `strict == Some(false)`”。

**修复后期望**：开关打开后返回 200，frontend 记一行 WARN（函数名、schema path、已降级为非 strict）。线上 E2 降到 0。

**PR（第 2 步）**：`fix(frontend): downgrade strict to false when strict-schema validation is skipped`

---

<a id="t17"></a>
### T17 请求体宽松反序列化（E3，cherry-pick + 上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | 49 次 / 天 |
| 代表性例子 | `openrouter:web_search` `e7c6e47e09a697e3428a9a45712581bb`；`tool_call` `89e4de7e3220e52ab964a38bee05732d`；`mid` `a1f21e079c06ea6f0e7f7d40259f087d`；untagged enum `9680a1fb8d0348360074c45edc4ba169`；duplicate `bc709e5185ee7b5deb91b23d40ea5435`；missing type `025169166fabd81987dc8e586811e383`；`"json_object"` `e555084858f4bbd9ee0c70bae6c003f0`；Responses 的 web_search `60ec25fa13166dfa612e1a6c0a46889a` |
| 上游状态 | duplicate `reasoning_content`：**main 已修**（cf6717581 #18213）。其余仍失败，main 用的 dynamo-protocols 10.0.1 只改了部分文案（例如 `expected function or custom`） |

**复现（L2，已实跑，与模型无关）**：`BASE=… ./repro_e.sh E3`（9 种子形态）。各 payload 见 `out/fix_repro/e-class/E_REPRO.md` §3，例如：

```bash
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","messages":[{"role":"user","content":"hi"}],"tools":[{"type":"openrouter:web_search"}]}'
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","reasoning_effort":"mid","messages":[{"role":"user","content":"hi"}]}'
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"'"$M"'","messages":[{"role":"user","content":"hi"}],"response_format":"json_object"}'
```

**修复前输出（实跑）**

```
400 {"message":"Failed to deserialize the JSON body into the target type: unknown variant `openrouter:web_search`, expected `function` at line 1 column 129",…}
```

frontend 日志只有一行 `http response sent status=400 latency_ms=0`。

**步骤**
1. **批次 1**：cherry-pick #18213（`from_slice_dedup_reasoning`）及其测试。
2. **批次 3**：上游 PR。只在类型化解析失败时，把 body 按 `serde_json::Value` 解析、做定向修正后再解析一次，成功路径零开销。
   - **无损修正，默认开**：tools 缺 type 时补 `"function"`；`tool_call` 改为 `function`；字符串形式的 response_format 改为对象；`mid` 改为 `medium`；user 的 `content:null` 改为 `""`；`input_text` 改为 `text`。
   - **有损修正，opt-in**：丢弃非 function 类型的 tool 并清理对应的 tool_choice；丢弃无法识别的 effort；system 消息只保留 text part；Responses 的 `web_search` 告警后跳过。
   - **继续拒绝**：file 类型的 part。

**修复前失败的单测**：`openai.rs` 仿照 `test_parse_chat_completion_request_escapes_control_chars_in_strings`（:6284），新增表驱动 `test_parse_chat_completion_request_lenient_repairs`。修复前全部 `Err`。

**修复后期望**：无损类返回 200，并记一行 `WARN Accepted request after lenient repair: <修正清单>`。有损类在开关关闭时仍返回 400，但带 T15 的 error 对象。

**PR**：`feat(frontend): lenient request normalization on deserialize failure`

---

<a id="t18"></a>
### T18 历史 tool_calls `arguments` 容错（E4，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | 22 次 / 天 |
| 代表性例子 | 控制字符 `61feff0aade76b4a38ac179a4ab9352a`（11-17 02:08:09）；字符串中途 EOF `c63d66b3474a764f3df7a1da5ef415b4`；对象中途 EOF `414672d9eb628dab7086dba30c4ca29d`；多余字符 `720c0e7837bc41e23917672049a5b90e`；不是对象 `01ff39704358b20c34d3acfd3d81b9a9` |
| main 是否仍存在 | 是（`validate.rs:476-525`） |

**复现（L2，已实跑，与模型无关）**

```bash
H='{"model":"'"$M"'","max_tokens":8,"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"f","arguments":'
T='}}]},{"role":"tool","tool_call_id":"c1","content":"ok"}]}'
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d "$H"'"{\"a\":\"x\ny\"}"'"$T"   # 控制字符
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d "$H"'"{\"a\":\"xyz"'"$T"        # 字符串中途 EOF
curl -s localhost:8000/v1/chat/completions -H 'Content-Type: application/json' -d "$H"'"{\"a\":1}}"'"$T"          # 多余字符
```

**修复前输出（实跑）**

```
400 {"message":"Validation: `messages[1].tool_calls[0].function.arguments` must be a valid JSON object string: control character (\\u0000-\\u001F) found while parsing a string at line 2 column 0",…}
```

**修复要点**
1. 先修复控制字符（默认开）。要修的是 **arguments 解码后的内层字符串**：现有的 `escape_json_string_control_chars`（`openai.rs:2748`）只处理外层 body，管不到这里。
2. 仍然失败时（opt-in）：记 `warn!` 后放行，把原字符串交给渲染器。
3. 对 JSON-object 模式的模型（`preprocessor/prompt.rs` 约 211-216），把 bail 改为替换成 `{}`。

**修复前失败的单测**：在 `chat_completions.rs:1492` 的 `test_validate_messages_rejects_bad_tool_call_arguments` 旁边新增两条：
- `test_validate_messages_repairs_control_chars_in_tool_call_arguments`；
- `…_tolerates_invalid_history_arguments_when_enabled`。

**修复后期望**：控制字符一类直接返回 200；其他几类在开关打开时返回 200，并记一行 WARN。

**PR**：`fix(frontend): tolerate malformed history tool-call arguments`（可以和 T21 合在一起）

---

<a id="t19"></a>
### T19 E5 补公开消息 + DSV4.1 `reasoning_effort` 映射（上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | 46 次 / 天中，有 44 次发生在 frontend 预处理阶段。另外 2 次见 T11 / T20 |
| 代表性例子 | 一个 org 在 03:15–03:20 连续 7 次 E5，body 只有 135–165B，中间夹着同一 org 唯一的一次 E6 `3899560a…`；另一个 org 有一个会话每轮都失败，body 从 1.68MB 长到 2.31MB（如 `c1aa0dc5…`） |
| 最可能的来源（待 T02 确认） | DeepSeek-V4.1 原生 formatter（dynamo-renderer 8.0.3）的 effort 只认 `none/low/high/max/1–100`，OpenAI 标准值 `medium/minimal/xhigh` 会让渲染失败，返回 `"Invalid request"`。main 依赖的 renderer 10.1.0 只补了 `medium` |
| 与模型无关的来源 | `tool_choice:"required"`（或具名）+ `guided_json`（`preprocessor/tool_choice.rs:90-97`） |
| 更正 FIX_PLAN | “tool_choice 指定了不存在的函数”**不会**产生 E5，HTTP 层会先返回带消息的 `Validation:` 400 |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/e-class && ./harness/start_dsv41.sh
curl -s localhost:18432/v1/chat/completions -H 'Content-Type: application/json' \
  -d '{"model":"deepseek-ai/DeepSeek-V4.1-Flash-1017","messages":[{"role":"user","content":"hi"}],"reasoning_effort":"medium"}'
# 与模型无关：
curl -s localhost:18431/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"Qwen/Qwen3-0.6B","max_tokens":8,"messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"get_weather","parameters":{"type":"object","properties":{}}}}],"tool_choice":"required","guided_json":{"type":"object"}}'
```

其他只在 DSV4.1 上触发的输入：`input_audio` / `video_url` part、文本里出现字面量 `<｜deepseek_image｜>`、`partial:true`、system 消息带 `tools`、`role:"function"`。用 `./repro_e.sh E5` 可以全部跑一遍。

**修复前输出（实跑，与线上逐字一致）**

```
400 {"message":"Invalid request","type":"Bad Request","code":400}
(debug) diagnostic=Some("DeepSeek V4.1 reasoning effort must be low, high, max, none, or an integer within 1–100")
```

**修复要点**
1. **effort 映射**：在 Dynamo 渲染之前，把 OpenAI 标准值映射到 DSV4.1 能接受的值，例如 `minimal→low`、`medium→62`（与 renderer 10.1.0 一致）、`xhigh→max`。具体映射在 PR 里和上游确认。也可以把依赖升级到 renderer 10.1.0 以上，但只能拿到 `medium`。
2. **公开消息**：
   - `map_prompt_render_error`（`preprocessor.rs:3387-3394`）对渲染器的 InvalidRequest 带出公开 message。这些都是渲染器的固定文案，不含用户内容；
   - `tool_choice.rs:91` 补上公开 message；
   - 配合 T15，带出 `param`。
3. 有了 T02 的统计后，按次数从高到低补其余来源。

**修复前失败的单测**
- `tool_choice.rs` 测试模块：`forced_tool_choice_with_guided_json_has_public_message`。
- `preprocessor.rs`：仿照 `continue_final_message_rejects_reordered_tool_results`（:8503），新增 `deepseek_v41_render_error_is_client_visible`，以及 `deepseek_v41_maps_openai_reasoning_effort`。

**修复后期望**：`reasoning_effort:"medium"` 返回 200。确实不支持的输入返回带具体原因的 400，例如 `"DeepSeek V4.1 native formatter supports text and image_url content only"`，并带 `param`。

**PR**
- `fix(preprocessor): map OpenAI reasoning_effort values for DeepSeek V4.1 formatter`
- `fix(preprocessor): expose prompt render and tool_choice errors as public messages`

---

<a id="t20"></a>
### T20 请求平面超限的固定文案改为公开（上游一行修复）

| 项 | 内容 |
|---|---|
| 解决 | E5 中 1 次。超大请求只拿到 `"Invalid request"`，不知道是请求太大 |
| 代表性例子 | `c5b40d629702e465270c1f15e8a71cf7`：body 30MB、81 张图。日志为 `TCP request validation failed … Request payload is too large for this deployment`（`tcp_client.rs:1691`） |
| 根因 | `lib/runtime/src/pipeline/network/egress/tcp_client.rs:1018-1024` 用 `.message(MAX_MESSAGE_SIZE_USER_MESSAGE)` 构造错误，这段文字因此成了私有诊断。常量名的意思就是给用户看的，应该用 `.public_message(...)`。main 相同（:1020）。同文件 `:51` 的 `INVALID_REQUEST_USER_MESSAGE`、`nats_client.rs:22` 也要一并检查 |
| 复现 | 代码推断。单测：构造超过 `max_message_size` 的请求，断言 `error.public_message() == Some(MAX_MESSAGE_SIZE_USER_MESSAGE)`；修复前为 None |
| 修复后期望 | 返回 400 `{"message":"Request payload is too large for this deployment",…}` |
| PR | `fix(runtime): return the request-plane size limit message to clients` |

---

<a id="t21"></a>
### T21 `thinking` 参数容错（E6，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | 1 次 / 天（同一 org 在试参数，见 T19） |
| 代表性例子 | `3899560a68096ccfd9f7cfe0846a7a3a`（11-17 03:20:18） |
| 复现（L2，已实跑） | `curl … -d '{"model":"'"$M"'","max_tokens":8,"thinking":"enabled","messages":[{"role":"user","content":"hi"}]}'` → ``400 {"message":"Validation: `thinking` must be a boolean or an object with `type` set to `enabled`, `disabled`, or `adaptive`",…}``；`"thinking":{"type":"auto"}` → ``Validation: `thinking.type` must be …`` |
| 位置 | `chat_completions.rs:250-273` `openai_thinking_mode`（main 在 :287-310） |
| 修复要点 | 接受字符串（enabled / disabled / adaptive / auto / true / false / on / off）、数字（0 为关，大于 0 为开），以及 `type:"auto"`。其他值记 WARN 后忽略 |
| 修复前失败的单测 | 在 `chat_completions.rs:1764` 旁新增 `test_openai_thinking_accepts_string_and_auto_forms` |
| 修复后期望 | 返回 200 |
| PR | 与 T18 合并为 `fix(frontend): tolerate malformed history tool-call arguments and thinking forms`，或者单独提 |

---

<a id="t22"></a>
### T22 frontend 连接开启 TCP keepalive（W5-1，上游 PR）

| 项 | 内容 |
|---|---|
| 解决 | frontend 一侧没有 keepalive：NLB 的空闲流（350s）会过期；客户端无声消失后，frontend 的 socket 一直不释放。**注意作用有限**：在已确认的碰撞模型里，被 RST 的是 frontend 一侧，真正半开的是 forward-worker 一侧。对 A1/B1/C* 对症的是 forward-worker 的 keepalive 和 `TCP_USER_TIMEOUT`（见 LINK_ANALYSIS 6.2） |
| 代表性例子 | 线上没有直接对应的单条 trace。相关数据：池化连接空闲 75–90s 后复用，死连接比例为 8.5%（LINK_ANALYSIS）；C1 `2a6fc286…` 这条连接复用前空闲了 21,966ms |
| main 是否仍存在 | 是。main 的 `server.rs:77-96` 对每条连接只设置了 `set_nodelay(true)` |

**复现（L2，已实跑）**

```bash
cd out/fix_repro/obs && ./repro_disconnect.sh w51
```

**修复前输出（实跑）**

```
t=1s   <srv>:8000  <cli>:60848                                         ← frontend：没有 keepalive 计时器
       <srv>:8001  <cli>:42780       timer:(keepalive,3sec,0)          ← 对照组（监听 socket 上设置 5/2/3）
t=60s  <srv>:8000  <cli>:60848                                         ← 半开连接仍然存在；对照组在 11.2s 时以 ETIMEDOUT 清掉
```

**修复要点**
- 必须同时设置 KEEPIDLE / KEEPINTVL / KEEPCNT。系统默认 `tcp_keepalive_time=7200`，只开 SO_KEEPALIVE 的话第一次探测要等 2 小时。
- 用环境变量控制，例如 `DYN_HTTP_TCP_KEEPALIVE_SECS`。
- **上游（main）**：在 `server.rs` 的 `accept()` 里，用 `socket2::SockRef::from(&stream)` 按连接设置。和 T04(b) 改的是同一处。
- **fork（1.6.1）**：在 `service_v2.rs:1205-1214` 的 `bind_listener` 里设置。Linux 上 accept 出来的连接会继承监听 socket 的设置，已实测。`lib/llm` 需要新增 socket2 依赖。

**修复前失败的单测**
- 1.6.1：`test_bind_listener_sets_tcp_keepalive`，放在 `service_v2.rs:3213` 旁边。
- main：`accepted_connections_enable_keepalive`，放在 `server.rs:147` 旁边。

**修复后期望**：`w51` 中 8000 的连接显示 `timer:(keepalive,…)`，半开连接在 idle + intvl×cnt 后被清理。线上可以执行 `nsenter -t <pid> -n ss -tno state established '( sport = :8000 )'` 检查。

**PR**：`feat(frontend): configurable TCP keepalive on accepted HTTP connections`

---

<a id="t23"></a>
### T23 向 SGLang / xgrammar 上游提 issue（外部）

| 项 | 内容 |
|---|---|
| xgrammar | 接受值为整数的浮点（`maxLength: 100.0`）。复现：`repro_f1.py` 的 F1-5，只需 `xgrammar==0.2.7`，`Grammar.from_json_schema('{"type":"object","properties":{"t":{"type":"string","maxLength":100.0}}}')` 就会报 `json_schema_converter.cc:4175: maxLength must be an integer` |
| SGLang | 给 `validate_xgrammar_json_schema` 加策略开关（reject / ignore / relax），并支持按请求区分严格程度。依据：F1-1～F1-4 这 4 种 schema，xgrammar 0.2.7 本身都能编译，只有预校验拒绝。复现：`repro_f1.py`（SGLang 原文件 + xgrammar） |
| SGLang（附带） | `base_processor.py:1047-1053` 的错误消息带 data URI 的前 100 个字符，会把用户媒体内容泄露到错误信息和日志里 |
| 结案 | issue 提交后记录链接；对方修复后，再评估能否撤掉 T14 的 F1-d 临时方案 |

---

## 4. 不在本清单内（非 Dynamo）

见 FIX_PLAN 第 8 节：
- **forward-worker**：连接池 idle ≤ 15s；开启 keepalive 和 `TCP_USER_TIMEOUT`；流式请求加响应头超时并重试。
- **网关**：400 不再统一标成 `unsupported_operation`。
- **NLB**：关闭 client IP preservation。

T01、T03 的取值都依赖 forward-worker 的超时设置，上线前要和网关同学对齐。

---

## 5. 对 FIX_PLAN 的更正汇总

| # | FIX_PLAN 原文 | 更正 | 影响 |
|---|---|---|---|
| 1 | P0-2(d) 只改 `Semantic request failure` 的日志级别 | E2/E3/E4/E6 的原因完全不进日志，E3 连 `request completed` 都没有，需要一起补 | T06 |
| 2 | P0-2(a) 的临时办法：调日志级别 | 调成 trace 后，会得到一个新的根 trace，更误导，不要用 | T04 |
| 3 | P0-2(b)：`into_make_service_with_connect_info` | 只适用于 1.6.1，且 HTTPS 分支 `:993-995` 也要改。main 换成了自有的 accept 循环，要改 `server.rs` | T04 |
| 4 | P0-2(e)：自己加 WARN | main 已修（58e83f6d2），cherry-pick 它的 Python 部分即可 | T07 |
| 5 | P0-2(f)：把上游 kill 降级 | 和上游 #12374 的测试冲突。要先区分取消来源 | T08 |
| 6 | E2：自己加开关；允许 `.` | 已在 main 修复（#16482、#17163），cherry-pick。“显式设 strict=false”仍需要另做 | T16 |
| 7 | E3：全部自己做 | duplicate `reasoning_content` 已在 main 修复（#18213） | T17 |
| 8 | E4：复用控制字符修复 | 要修的是 arguments 解码后的内层字符串，不是原始 body | T18 |
| 9 | E5 的候选来源 | 漏了最可能的 DSV4.1 effort 映射和 `tool_choice.rs:91`。“tool_choice 指定不存在的函数”不会产生 E5。46 次中有 2 次不是 frontend 校验（prefill 返回的 F2 类错误 1 次，请求平面超限 1 次） | T19 T20 |
| 10 | E5：`"Invalid request"` 来自 `error.rs:61` | 应为 `lib/llm/src/http/service/error.rs:61`，不是 `lib/runtime/src/error.rs` | — |
| 11 | F1-a：`strict_schema.rs` 把 100.0 当整数，所以 Dynamo 放行 | `strict_schema.rs` 只管 `tools`，frontend 对 response_format 根本不做 schema 校验 | T13 |
| 12 | F1：只说是 xgrammar 不支持 | F1-1～F1-4 xgrammar 0.2.7 都能编译，只有 SGLang 的预校验拒绝。F1-5 才是 xgrammar 自己报错 | T14 T23 |
| 13 | F4：“超出五十多万 token” | 只有 1 条如此（523,837），另外 2 条分别超出 186,178 和 26,702 | T09 |
| 14 | F5：“先找原因” | 已确认：decode `Requested token count exceeds …`，5/5 对上。日志不带 trace_id，要按时间窗查 | T09 T10 |
| 15 | F5：更新测试 `decode_failure_does_not_wait_for_prefill` | 这只适用于 1.6.1。main 上要改的是 `other_invalid_argument_decode_errors_preserve_prefill_transfer`，需要先讨论 | T10 |
| 16 | P0-1：窗口 2000–3000ms | 线上最长 516ms，1000ms 就够 | T01 |
| 17 | W5-1：“及时清理半开连接” | 只能清理 frontend 一侧。线上半开的是 forward-worker 一侧，所以对链路类错误的直接收益有限 | T22 |
| 18 | W5-2：没写和 P0-1 的关系 | P0-1 窗口期内连响应头都不发，注释帧也不会发 | T03 |
| 19 | SGLang 行号 | 线上镜像的 `tokenizer_manager.py` 比 PyPI v0.5.23 多约 15 行，改补丁时按函数名定位 | — |
| 20 | 没提 F2 错误消息的隐私问题 | SGLang 的消息带 data URI 前 100 个字符（约 58 字节客户内容），日志和 issue 都要脱敏 | T05 T11 T23 |
