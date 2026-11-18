# Dynamo 修复计划：deepseek-v4.1-flash-1017_4

- **依据**：同目录的 `REPORT.md`（第二版，2026-11-17）。时间窗为 UTC 2026-11-16 03:37 → 11-17 03:37，`_4` 共 3,644 个失败事件。
- **代码基线**：
  - Dynamo：`/workspace/codes/dynamo`，分支 `gmi/1.6.1-sglang-v0.5.23`（HEAD `06298b4b0`）。
  - SGLang：v0.5.23（xgrammar 0.2.7）。
  - 下文路径：Dynamo 写相对仓库根目录的路径；SGLang 写 `sglang/srt/...`。行号基于上述版本，标“约”的行号改代码前请以函数名为准重新定位。
- **说明**：本文是 GMI fork 内部的分析记录。文中 `out/…` 开头的路径指分析工作区 `obs-console-playground` 中的数据与脚本，未随本文提交。文中的地址和客户标识已脱敏：NLB 的两个 IP 记为 NLB-IP-1、NLB-IP-2，NLB 主机名记为 `nlb.example`，控制台记为 `obs-console.example`，forward-worker 的 pod IP 记为 pod-A1 这样的代号（字母相同表示 /24 网段相同；两个区域的网段有重叠，区域也相同才是同一节点），集群名只保留区域，客户 org ID 已删除。
- **范围**：Dynamo 自身，包括 frontend、SGLang worker 集成，以及 SGLang 本身。链路问题（约 80% 的失败）只列 Dynamo 侧能配合的部分。
- **执行版（2026-11-18）**：同目录的 `FIX_TASKS.md` 把本计划拆成 T01–T23 任务卡，含代表性例子、实跑复现和 issue/PR 草稿；复现材料在 `out/fix_repro/`。两者冲突时以 FIX_TASKS 为准，本文需要更正的 20 处见 FIX_TASKS 第 5 节（例如 E2/E3e/P0-2(e) 上游 main 已修，P0-2(f) 与 F5 取消 prefill 和上游现有设计冲突）。

---

## 1. 归因回顾：Dynamo 要负责的部分

| 组 | 子类 | 次数/天 | 位置 | 本计划对应 |
|---|---|---:|---|---|
| frontend 请求校验 / 兼容性 | E1 134 · E2 57 · E3 49 · E5 46 · E4 22 · E6 1 | 309 | Dynamo frontend（Rust HTTP，路由之前） | W2 |
| 结构化输出 | F1 144 | 144 | SGLang xgrammar 预校验；Dynamo 原样透传 schema | W3 |
| 多模态 | F2 125 · F3 25 · F4 3 · F5 5 | 158 | frontend 不校验媒体、对含图请求跳过长度预检；SGLang 解码；decode 拒绝后 prefill 未取消 | W4 |
| **合计** | | **611**（16.8%） | | |

另有约 80% 的失败（2,873 次及 A3 的大部分）是 forward-worker 与 Dynamo frontend 之间的**连接中断**。prefill 的 `KVTransferError` 只是 decode 被取消后的副作用，Dynamo 本身没有出错（证据见 REPORT 第 2 节）。Dynamo 侧能做的是：补可观测性，帮助确认链路根因（W1）；以及加强连接保活（W5）。

还有三个横向问题，会放大以上所有错误：

1. frontend 默认**先提交 HTTP 200，再等 worker 报错**（pre-commit peek 关闭）。所以 F 类错误只能以流内错误帧返回。
2. 多处错误**只记录类型、不记录原因**：路由失败、语义校验失败、客户端断开。
3. 返回给调用方的错误体不是 OpenAI 标准格式（`type: "Bad Request"`，消息常常只有 `"Invalid request"`）。

---

## 2. 工作项总览

| 编号 | 工作项 | 解决 | 次数/天 | 类型 | 主要位置 | 去向 | 工作量 |
|---|---|---|---:|---|---|---|---|
| **P0-1** | 开启 pre-commit 错误窥探 | F1–F5 从“200 + 流内错误帧”变为直接返回 400 | 302 | 配置 | `DYN_HTTP_PRE_COMMIT_ERROR_PEEK_MS` | fork 配置 | S |
| **P0-2** | 可观测性补强（6 个小点） | 链路根因取证；E5 / F5 的原因可见 | — | 小代码 | disconnect.rs、route_span.rs、openai.rs、logging.rs、service_v2.rs、prefill_handler.py | 上游 PR | S×6 |
| **W2** | frontend 请求兼容性 + OpenAI 标准错误体 | E1–E6 | 309 | 代码 | openai.rs、validate.rs、strict_schema.rs、chat_completions.rs、preprocessor.rs | 上游 PR（宽松行为加开关） | M–L |
| **W3** | 结构化输出 schema 兼容 | F1 | 144 | 代码 | preprocessor.rs + 新模块；可选 SGLang 插件 | 上游 PR + SGLang 上游 | S–L |
| **W4** | 多模态输入校验、长度预检、取消 prefill | F2–F5 | 158 | 代码 | preprocessor.rs、prefill_router/handoff.rs、admission.rs；SGLang worker | 上游 PR | M–L |
| **W5** | 连接保活 / 探测 | 配合链路问题 | （2,873 的一部分） | 小代码 + 配置 | service_v2.rs `bind_listener`；SSE keep-alive | 上游 PR + 配置 | S |

**建议顺序**

1. **P0-1、P0-2**：零代码或很小的改动，马上改善状态码，并拿到定位所需的信息。
2. **W4 的 F2/F3 输入嗅探**：涉及 49 个 org，影响面最广。
3. **W3 的 F1-a 整数化**：成本低。
4. **W2**：按次数依次处理 E1 错误体、E2、E3、E5。
5. **W4 的 F4/F5 长度预检和取消 prefill**。
6. **W3 的 F1-b / F1-c**：需要先确认请求里的 `strict` 取值。
7. **W5**：与链路验证配合进行。

---

## 3. P0：本周可做

### P0-1 开启 pre-commit 错误窥探（纯配置）

- **现状**：`DYN_HTTP_PRE_COMMIT_ERROR_PEEK_MS` 未设置，即 `BackendErrorCheck::Skip`（`lib/llm/src/http/service/service_v2.rs:226-276`）。frontend 拿到 engine stream 后立刻提交 200。worker 稍后报的 400（加载图片、编译 grammar、decode 拒绝）只能以 SSE 错误帧返回，网关看到的是“200 后断流”。
- **改动**：frontend 设置 `DYN_HTTP_PRE_COMMIT_ERROR_PEEK_MS=3000`，可从 2000–3000 开始。仓库文档（`docs/fern/pages/recipes/model-recipes/gemma-4-31b.mdx:289`）也推荐用它解决“多模态流式请求状态码误报”。
- **机制**：`lib/llm/src/http/service/openai.rs` 的 `check_for_backend_error_info`（约 3046-3096；chat 调用点在 3642）在窗口内等待第一个非 annotation 事件。如果是错误，就映射成对应 HTTP 状态（400）返回；否则照常进入 SSE。
- **代价**：流式请求的响应头延迟到“首个 token”和“窗口”二者中较早的那个；TTFT 本身不变。forward-worker / 网关的响应头超时必须大于“窗口 + frontend 预处理耗时”。
- **验收**：F 类 `vendor_error_frame/invalid_request/400` 显著下降；frontend 出现 `http response sent status=400`。

### P0-2 可观测性补强（每项都很小，适合上游 PR）

| # | 问题 | 位置 | 改法 |
|---|---|---|---|
| a | `WARN Stream closed unexpectedly` 没有 trace_id，按 trace 查日志查不到 | `lib/llm/src/http/service/disconnect.rs:305`（`create_connection_monitor` 里的 `tokio::spawn`）；`:320` 的 `#[tracing::instrument(level = "trace", …)]` | spawn 时加 `.instrument(tracing::Span::current())`，让 monitor 继承请求 span；`warn!` 中补充 request_id、距请求开始的耗时、是否已开始输出 |
| b | 不知道断开的是哪条连接 | `service_v2.rs:1073` 的 `axum::serve(listener, router)`；`lib/runtime/src/logging.rs:623` 的 `make_inference_request_span` | 改为 `router.into_make_service_with_connect_info::<SocketAddr>()`；在 span 中从 `req.extensions().get::<ConnectInfo<SocketAddr>>()` 记录 `client.address` / `client.port`。可用来检查同一时刻是否有两条连接来自相同的源 ip:port（REPORT 2.5 节的 NLB 假设）。注意：经过 NodePort 时看到的地址取决于 `externalTrafficPolicy` |
| c | `route request failed` 只记 `error.type`（F5 的 decode 拒绝原因因此不可见） | `lib/runtime/src/pipeline/network/egress/route_span.rs:297` 的 `finish_error`；调用方是 `poll_next`（约 :355） | 给 `finish_error` 增加错误消息参数（截断并脱敏，符合 `lib/runtime/src/error.rs` 约 41-45 的公开策略），一起写进日志 |
| d | 语义校验失败的真实原因只在 debug 级别记录（E5 因此不可见） | `lib/llm/src/http/service/openai.rs:792-798`（`Semantic request failure`） | 对 InvalidRequest 类改用 `warn!`，带 class / reason / diagnostic。**不改代码的临时办法**：`DYN_LOG="info,dynamo_llm::http::service::openai=debug,dynamo_llm::preprocessor=debug"` |
| e | SGLang prefill 的中止原因（如 bootstrap 超时）被吞掉 | `components/src/dynamo/sglang/request_handlers/llm/prefill_handler.py` 的 `_consume_results`（约 295-336） | `finish_reason` 为 abort 时记 WARN（rid、原因、耗时），并考虑作为错误返回给 frontend |
| f | 客户端断开后 decode 报 ERROR `Failed to publish complete final for stream`，误导分析 | `lib/runtime/src/pipeline/network/ingress/push_handler.rs:380-384` | 已经区分了 peer `Stop` 的情况（:369-375）。对上游 `kill`（客户端断开）同样降级为 info / debug，但保留真实连接故障的计数 |

**验收**：`Stream closed unexpectedly` 能按 trace_id 查到，并带 `client.address`；F5 一类的 decode 拒绝在日志中有具体原因；E5 的 46 个能归到具体来源。

---

## 4. W2：frontend 请求兼容性（E1–E6，309 次/天）

### 错误体的两条生成路径

- **A：普通校验错误**。由 `openai.rs` 的 `from_http_error`（约 908-936）生成，消息原样带出（加 `"Validation: "` 前缀）。
- **B：类型化的 DynamoError**。由 `from_semantic_error_with_recording`（约 765-820）生成。message 取错误的公开 message，没有时回落为 `error.rs:61` 的 `"Invalid request"`；`details` 取结构化的公开细节。

两条路径的 `type` 都是 HTTP 原因短语（`"Bad Request"`）。

### W2-0 OpenAI 标准错误体（E1 / E5 的前提）

- **位置**：`openai.rs` 的 `ErrorMessage`（约 240-250，derive Serialize）。
- **改法**：手写 Serialize。保留现有的 `message` / `type` / `code` / `details`，**另外增加**嵌套的 `"error": {message, type, param, code}`：
  - `type`：复用 `disconnect.rs`（约 393-411）中流式错误已有的状态码映射，例如 400 → `invalid_request_error`。
  - `code` / `param`：从 `details` 推导，例如 context_length → `context_length_exceeded` / `messages`。
- **兼容性**：openai-python SDK 优先读取 `body["error"]`。现有测试断言的顶层 `"type":"Bad Request"` 不受影响。

### E1 上下文超长（134）

- **位置**：`lib/llm/src/preprocessor.rs` 中：
  - `validate_requested_token_budget`（约 1877-1926）：prompt + max_tokens 超限的分支在约 1896-1922。
  - `prompt_overflow_error`（约 4811-4825）：prompt 本身超限。
  - 两者都只附带 `PublicDetails::ContextLength{limit, actual}`，没有公开 message。
- **改法**：
  1. 在 `from_semantic_error_with_recording`（openai.rs 约 801-819）中，当没有公开 message、而 details 是 ContextLength 时，用数字拼出 OpenAI 风格的消息（“This model's maximum context length is {limit} tokens. However, you requested {actual} tokens…”），配合 W2-0 的 `code`。
  2. 可选（opt-in）：如果只是 prompt + max_tokens 超限、prompt 本身没超，就把 max_tokens 截短而不是报错。注意这与 OpenAI / vLLM 的行为不同。
- **说明**：1,048,576 是模型上限，这类请求本身无法放行。本项的目标是让错误能被识别，次数不会下降。

### E2 tools strict schema / 函数名（57）

- **位置**：
  - `lib/llm/src/protocols/openai/strict_schema.rs:34-46`：只在 `strict == Some(true)` 时校验。禁用关键字列表约在 13-27，`additionalProperties` 检查约 376-381，`required` 检查约 405-414。
  - 函数名检查：`lib/llm/src/protocols/openai/validate.rs:728-741`，按字节只允许 `[A-Za-z0-9_-]`。
  - 调用链：`chat_completions.rs` 的 `validate()` → `validated_effective_tools` → `validate_tools`（约 702-756）。preprocessor 中的 `effective_tools`（`preprocessor/tool_choice.rs` 约 251-259）还会再校验一次。
  - 这是 1.6.0 有意引入的行为（`docs/fern/pages/use-cases/tool-calling-and-reasoning/structural-tag.md` 约 223-251）。
- **改法**（加 env 开关，仿照已有的 `DYN_IGNORE_OPENAI_FE_UNSUPPORTED_FIELDS`，validate.rs:132-165）：
  - strict 校验失败时记 `warn!`，并把该 tool 的 `strict` **显式**设为 `Some(false)`。必须显式设为 false，因为 structural tag 会把未设置当作 true。放在 `openai.rs` 的 thinking 归一化之后（约 3509-3512），确保在两处校验之前生效；system message 中携带的 tools 一并处理。
  - Responses API：`protocols/openai/responses/mod.rs` 约 842-845 处同样处理。
  - 函数名放宽，允许 `.`（validate.rs:734）。
- **风险**：降级为非 strict 后，参数不再受 grammar 约束；`.` 在 tool parser 和模板中的兼容性需要测试。
- **测试**：`lib/llm/tests/frontend_protocol_validation_http.rs`（约 342-405），以及 `chat_completions.rs` 的单测（约 1672-1690）。

### E3 请求体反序列化失败（49）

- **位置**：`openai.rs` 的 `parse_json_request`（约 2606-2631）失败后进入 `json_deserialize_error`（约 2652-2660）。类型来自外部 crate `dynamo-protocols =8.2.0`（基于 async-openai 0.42.1），不在仓库内，**不建议改 serde 属性**（需要 fork crate）。
- **改法**：只在类型化解析失败时，把 body 解析为 `serde_json::Value`，做定向修正后再解析一次，并用 `warn!` 记录改了什么。成功路径零开销。现有代码在这里已经有“转义控制字符 / 修复 UTF-8 后重试”的先例（约 2633-2650、2748-2780）。
  - **无损修正（默认开）**：
    - tools 缺 `type` 时补 `"function"`；
    - `tool_calls[].type: "tool_call"` 改为 `"function"`；
    - 字符串形式的 `response_format` 改为 `{"type": …}`；
    - 同时出现 `reasoning` 和 `reasoning_content` 时保留后者；
    - user 消息 `content: null` 改为 `""`；
    - `input_text` / `output_text` 类型的 part 改为 `text`；
    - `reasoning_effort` 同义词映射（`mid` → `medium`）。
  - **有损修正（opt-in）**：
    - 丢弃非 function 类型的 tool（如 `openrouter:web_search`），并清理指向它的 `tool_choice`；
    - 丢弃无法识别的 `reasoning_effort`；
    - system 消息只保留 text part；
    - Responses API 的 `web_search`（`responses/mod.rs` 约 742/746）改为告警后跳过。
  - **继续拒绝**：file / document 类型的 part。
- **风险**：有损修正会悄悄改变行为（例如 web search 实际上不会执行）；请求体最大 45MB，失败路径会多解析一次。

### E4 历史 `tool_calls[].function.arguments` 非法（22）

- **位置**：`validate.rs` 的 `validate_messages`（约 544-568）→ `validate_json_object_string`（约 570-580）。这个检查对所有模型的历史消息都生效（最初为 MiniMax-M3 引入）。
- **改法**：
  1. 先修复原始控制字符（复用 openai.rs 约 2748-2780 的逻辑，移到公共工具函数），可以无损解决 control character 这一类。
  2. 仍然失败时（加 env 开关）记 `warn!` 后放行，原字符串交给渲染器。dynamo-renderer 解析失败时会保留原字符串。
  3. 对 JSON-object 模式的模型（`preprocessor/prompt.rs` 约 211-216），把 bail 改为替换成 `{}`。
- **风险**：需要 dict 的模板如果不做第 3 步仍会失败。

### E5 `Invalid request` 没有细节（46）

- **位置**：`from_semantic_error_with_recording`（openai.rs 约 765-820）。错误既没有公开 message 也没有 details 时，回落为 `"Invalid request"`；真实原因只在 debug 级别记录（792-798）。
- **候选来源**（都是通过 `preprocessor.rs:134-142` 的 `invalid_argument_error` 生成的私有错误）：
  - chat template 渲染失败（`map_prompt_render_error`，preprocessor.rs:3387-3394）；
  - tool_choice guidance（`tool_choice.rs` 约 167 / 258 / 312 / 347）；
  - structural tag（`structural_tag.rs` 约 93-108）；
  - 媒体 URL 策略（`media/loader.rs` 约 134-140）。
- **改法**：
  1. 先做 P0-2(d)，确认到底是哪个来源。
  2. 对确认的来源定向补充公开 message，例如模板渲染错误（vLLM 也会返回模板错误文本）和 tool_choice 错误。
  3. 或者把 `invalid_argument_error` 改成与 `protocols/common.rs`（约 48-56）一样带公开 message。这会影响约 20 处调用，需要逐个确认不会泄露敏感信息。
- **风险**：diagnostic 里可能有预签名 URL、模板路径、用户文本。运行时约定（`lib/runtime/src/error.rs` 约 41-45）不允许直接把 diagnostic 返回给调用方，所以只能定向补充或做成 opt-in。

### E6 `thinking` 参数（1）

- **位置**：`chat_completions.rs` 的 `openai_thinking_mode`（约 250-273）。
- **改法**：接受字符串（enabled / disabled / adaptive / auto / true / false / on / off）、数字（0 表示关，大于 0 表示开），以及 `type: "auto"`；其他值记 `warn!` 后忽略。风险低。

---

## 5. W3：结构化输出（F1，144 次/天，全部来自同一个 org）

**机制**

- SGLang v0.5.23 的 xgrammar 后端在编译前无条件执行 `validate_xgrammar_json_schema`：
  - 定义在 `sglang/srt/constrained/json_schema_validation.py:167-200`；
  - 调用点是 `sglang/srt/constrained/xgrammar_backend.py` 的 `dispatch_json`（约 384-408，第 393 行）；
  - 拒绝的是 xgrammar 0.2.x 会静默忽略的约束，以及无法同时满足的约束组合。
- prefill 和 decode 都会编译 grammar，都会以 400 中止请求。
- Dynamo 原样透传 `response_format.json_schema.schema`（`lib/llm/src/protocols/openai/chat_completions.rs:463-488` 的 `get_guided_json`），**不读取 `strict`**。
- `maxLength must be an integer` 是另一个问题：xgrammar 要求这类字段是整数，`100.0` 这样的浮点就会失败。Dynamo 从 serde → pythonize → `json.dumps` 一路保留了浮点形式。

**修复项**

| 项 | 内容 | 位置 | 说明 |
|---|---|---|---|
| F1-a（必做，无损） | 把 minLength / maxLength / minItems / maxItems / minProperties / maxProperties / minContains / maxContains 中“值为整数的浮点数”（如 `100.0`）转成整数，并去掉 null | 生成 guided json 的地方（`get_guided_json`，或 `preprocessor.rs` 约 3193 写入 `sampling_options.guided_decoding.json` 处）；tool 参数 schema 同样处理 | `strict_schema.rs` 约 333-334 已把 `100.0` 当作整数，所以 Dynamo 放行、xgrammar 却拒绝。也适合同时向 xgrammar 上游提出“接受整数值的浮点数” |
| F1-b（strict 不为 true 时宽松化，opt-in + 能力开关） | 去掉 xgrammar 不支持的 11 个关键字（contains、dependentRequired、dependentSchemas、else、if、maxContains、minContains、multipleOf、not、then、uniqueItems）和不认识的 format；`format`+`pattern` 时，format 是 xgrammar 支持的就保留 format，否则保留 pattern；`pattern`+长度约束时保留 pattern（pattern 含 lookaround、反向引用、`\b`、`\p` 时除外）；记录被移除的 JSON pointer 并计数 | 新模块，放在 `protocols/openai/strict_schema.rs` 旁；在 `preprocessor.rs` 约 7659-7663 调用（此处同时拿得到 `response_format.strict` 和 guided json）；由 worker 通过 `set_engine_specific` 发布能力开关（`components/src/dynamo/sglang/register.py` 约 542-553） | 只修改 grammar 用的副本，chat template 仍然看到原始 schema（`preprocessor/prompt.rs` 约 269-274）。只覆盖默认的 `dynamo` chat processor |
| F1-c（strict 为 true 时在 frontend 预校验） | 用 Rust 移植 `validate_xgrammar_json_schema`，错误信息保持一致，在提交 200 之前就返回 400 | `preprocessor.rs` 约 3178-3191（已有 SGLang 专用的 `invalid_argument_error` 处） | 不要复用 `strict_schema.rs`，那是 OpenAI strict 子集，规则不同 |
| F1-d（fork 临时方案） | 通过 SGLang 插件入口 `sglang.srt.plugins`，给 `XGrammarGrammarBackend.dispatch_json` 加 around hook，先宽松化再编译；最简单的版本是把预校验改成只告警 | Dynamo 已有先例：GMS 插件（`lib/gpu_memory_service/pyproject.toml` 约 41-42）；主 `pyproject.toml` 需新增 entry point，并用环境变量控制 | 依赖 SGLang 私有实现，每次升级都可能失效；看不到 `strict`。**不要**注册新的 grammar backend 名字：那样会绕过 reasoning 模型的包装，也会破坏 `grammar_backend == "xgrammar"` 的判断 |

- **不推荐整体切到 `--grammar-backend llguidance`**：
  - 它能处理 format+pattern、pattern+长度；
  - 但默认仍会拒绝 uniqueItems、if/then（需要在 schema 里注入 `x-guidance: {lenient: true}`），`maxLength: 100.0` 仍然失败；
  - 不支持 uri-reference 等 format；reasoning 模型的 answer framing 也用不了；
  - prefill 和 decode 必须同时切换。
- **上游方向**：
  - SGLang：给预校验加策略开关（reject / ignore / relax），并支持按请求区分严格程度。
  - xgrammar：接受值为整数的浮点。
- **先确认**：这个 org 的请求里 `json_schema.strict` 是什么，它决定走 F1-b 还是 F1-c。可以用 `gmi-sre logs for-trace --include-request-data` 只看这一个字段（日志包含客户内容，注意权限）。
- **组合建议**：F1-a 必做；strict 不为 true 走 F1-b，strict 为 true 走 F1-c；过渡期依靠 P0-1 让错误至少以 400 返回。

---

## 6. W4：多模态（F2–F5，158 次/天）

**现状**

- 本部署走 URL 透传：frontend 既不校验 MIME，也不解码图片（只为 mm-routing 读取图片尺寸，失败只打 WARN），原始 `data:` URI 同时发给 prefill 和 decode，两侧各解码一次。
- SGLang 的 `get_image_bytes`（`sglang/srt/utils/common.py:2195-2197`）直接丢弃 MIME，把数据交给 Pillow。
- Dynamo 的 Python `ImageLoader`（`components/src/dynamo/common/multimodal/image_loader.py` 约 227-233）其实已有“Data URL must be an image type”的检查，但 SGLang 分离式部署的路径不经过它。

### F2/F3 在 frontend 嗅探媒体真实格式（150，涉及 49 个 org）

- **位置**：`lib/llm/src/preprocessor.rs` 中逐个处理 content part 的循环里，紧跟在 data URL 大小检查之后（约 3663-3681）。
- **改法**：
  1. 对 `image_url` 的 data URI，只解码前几十个字符的 base64，然后识别格式：用 `image::guess_format`，再加上显式识别 HEIC（`ftyp` + heic / heix / mif1 / msf1）、`%PDF-`、RIFF/WAVE、ID3、OggS、fLaC。
  2. 只放行 SGLang 镜像里 Pillow 能处理的格式：JPEG / PNG / GIF / WEBP / BMP / TIFF（做了 HEIC 支持后再加 HEIC）。
  3. 其他格式通过 `lib/llm/src/protocols/common.rs`（约 48-56）返回一条固定的公开 400 消息，例如“image_url 只支持图片，收到的是 audio/wav”。
  4. **按魔数判断，不按声明的 MIME 判断**，这样 MIME 标错、但内容确实是图片的请求依然可用。
- **注意**：
  - 这项检查必须始终执行：不依赖 `mm-routing` feature，也不要在请求带 user uuid 时跳过（约 3872）。
  - HTTP URL 不下载就无法检查，靠 P0-1 兜底。
  - F3 的请求同时记录解码后数据的前 16 个字节（hex），用来确认真实格式（怀疑是改了扩展名的 HEIC、SVG 或 HTML）。

### HEIC 与 PDF（F2 中各 1 个和 32 个）

- **HEIC**：SGLang runtime 镜像加装 `pillow-heif`；在 Dynamo SGLang worker 创建 `sgl.Engine` 之前调用 `register_heif_opener()`（例如 `components/src/dynamo/sglang/init_llm.py` 或 `main.py`），prefill 和 decode 都要做。需注意 HEVC 许可问题和 libheif 的攻击面。
- **PDF**：建议直接返回明确的 400。要支持 PDF，需要逐页转成图片，并限制页数、DPI、做沙箱隔离，工作量大。
- **音频**：DeepSeek-V4.1 不支持音频。
  - 可以把 SGLang 计算出的模态能力（`sglang/srt/configs/model_config.py` 约 816-846：`is_multimodal` / `is_audio_model` / `is_image_understandable_model`）从 `register.py` 作为 engine-specific key 发布出来，frontend 据此拒绝模型不支持的模态。
  - 真正的 `audio_url` 在端到端链路上是否会被静默丢弃，需要另外确认。

### F4 含图请求的长度预检（3；也可能是 F5 的原因）

- **位置**：`preprocessor.rs` 的 `exact_prompt_len`（1849-1874）在有图片、但拿不到 MM 展开长度时返回 None，于是 `validate_preprocessed_token_budget`（约 1930-1955）整个跳过。DeepSeek-V4.1 不在 MM routing 的 prompt layout 支持列表里，所以永远拿不到展开长度。
- **改法**：
  1. **下界检查（成本低，建议先做）**：含图片但没有展开长度时，仍用 `token_ids.len()` 检查。图片占位符展开后只会变长，所以 `token_ids.len() >= 上限` 一定是超限。`reject_total_overflow` 为真时，同样检查 `token_ids.len() + max_tokens`。
  2. **精确计数（进阶）**：为 DeepSeek-V4.1 实现 `ImageRoutingBackend`（`lib/llm/src/preprocessor/mm_routing/image.rs` 约 111-145、180-223，可参照 Nemotron 的实现）。按 `vision_config`（patch_size、downsample_ratio、max_image_tokens 等）计算每张图的 token 数。计算结果必须与 SGLang（`sglang/srt/multimodal/deepseek_v41_image_processing.py`）**完全一致**，否则会误报 400。

### F5 decode 拒绝：先找原因，再修 prefill 泄漏（5）

- **找原因**：P0-2(c) 完成后，decode 的拒绝消息就能看到。最可能的原因是 SGLang 的总 token 检查：decode 使用调用方的 `max_tokens`，prefill 只用 1，所以“输入 + max_tokens 超出上下文”（`sglang/srt/managers/tokenizer_manager.py` 约 1353-1378）只会在 decode 侧触发。F4 的总量检查可以在 frontend 把它拦住。其他可能：
  - decode 的 KV 池比 prefill 小（`sglang/srt/managers/tp_worker.py` 约 550-572 计算 `max_req_input_len`）；
  - decode 侧的 `--limit-mm-data-per-request` 配置不同；
  - 只有 decode 侧取图失败。
- **修 prefill 泄漏**：
  - `lib/llm/src/kv_router/prefill_router/admission.rs`（约 79-89、150-168）：让 `PrefillTask` 持有 prefill 的 context。
  - `handoff.rs` 的 `forward_decode`：在 decode 派发失败（:92 的 `result?`），或 decode 首帧就是错误 / `FinishReason::Error`（:114-123）时，调用 prefill context 的 `stop_generating()`。
  - **调用方断开时不要取消 prefill**，保持现有约定（`prefill_router/mod.rs` 约 595-606；测试 handoff.rs:320、mod.rs 约 1066-1088）。需要更新的测试是 `decode_failure_does_not_wait_for_prefill`（handoff.rs:271）。
  - 取消信号会经 TCP 控制消息到达 worker（`components/src/dynamo/sglang/request_handlers/cancellation.py` 约 156-174、246-286），SGLang 会中止仍在 bootstrap 队列中的请求（`sglang/srt/managers/scheduler.py` 约 5742-5750）。
  - worker 已经发布了能力 `disagg_prefill_cancel_anytime_v1`（`register.py` 约 509-523），但 Rust 侧目前没有读取，可以用它作为开关。
- **缓解**：适当调低 prefill 的 `SGLANG_DISAGGREGATION_BOOTSTRAP_TIMEOUT`（默认 300，`sglang/srt/environ.py:759`）。风险是 decode 排队较长时，会误杀正常但较慢的 bootstrap。

---

## 7. W5：链路配合（Dynamo 侧）

- **W5-1 frontend 监听 socket 开启 TCP keepalive**：
  - 位置：`service_v2.rs` 的 `bind_listener`（1205-1215）。
  - 在 Linux 上，监听 socket 设置的 SO_KEEPALIVE / TCP_KEEPIDLE / TCP_KEEPINTVL / TCP_KEEPCNT 会被 accept 出来的连接继承。用 `socket2` 设置（`lib/runtime` 已经依赖它，`lib/llm` 需要新增），通过环境变量控制（如 `DYN_HTTP_TCP_KEEPALIVE_SECS`）。
  - 如果不想依赖继承行为，也可以在 accept 之后对每个连接单独设置（需要自定义 accept 循环，或者使用所用 axum 版本提供的连接钩子）。
  - 作用：空闲的 keep-alive 连接会被定期探测，刷新 NAT / NLB 上的连接状态，并及时清理半开连接。
  - 局限：解决不了“新连接撞掉老连接”这类问题。
- **W5-2 SSE keep-alive 配合网关空闲读超时**：
  - 设置已有的 `DYN_HTTP_SSE_KEEP_ALIVE_INTERVAL_MS`（例如 5000）后，等待首个 token 期间也会定期发送 SSE 注释帧。
  - 配合 forward-worker / 网关的“N 秒内没收到任何字节就判定连接失效”（例如 20s），可以把 C/B 类的 120s 等待缩短到约 20s，同时不会误杀长 prefill。
  - 前提：网关 adapter 能正确忽略 SSE 注释帧（以 `:` 开头的行）。
- **W5-3 链路根因验证**（不涉及 Dynamo 代码）：见 REPORT 2.5 节。P0-2(a)(b) 的日志是这一步的关键证据。

---

## 8. 非 Dynamo 配合项（简列）

- **forward-worker**：
  - 连接池的 `IdleConnTimeout` 设为不超过 15s，并开启 TCP keepalive；
  - 流式请求加响应头超时（必须大于 P0-1 的窗口加上预处理耗时，例如 15s），超时后在新连接上重试。
- **网关**：Dynamo 返回的 400 不要统一标成 `unsupported_operation`。应按 vendor_body 或错误帧映射错误类型，记为 `client_fault`，不计入 endpoint 健康分，并把 message 透传给调用方。
- **NLB / NAT**：按 REPORT 2.5 节验证 client IP preservation 的假设。

---

## 9. 上游 PR 拆分建议

仓库约定（`AGENTS.md`）：

- PR 标题用 Conventional Commit 格式 `type(scope): summary`；
- 每个 commit 都要 DCO 签名（`git commit -s`）；fork 提交的 PR 要想自动跑 CI，commit 签名需要显示为 Verified；
- PR 需要关联一个 issue（`Closes #…`），描述里要有 `Summary` 和 `Validation`；
- 提交前运行 `cargo fmt --all && cargo clippy --workspace`、`pre-commit run --all-files`；测试用 `cargo test -p dynamo-llm`、`pytest -m unit tests/`；
- 架构层面的改动需要先走 DEP。

| # | 建议标题 | 内容 | 开关 | 备注 |
|---|---|---|---|---|
| 1 | `fix(frontend): attach request context and peer address to disconnect logs` | P0-2 a、b | 否 | 小改动，最先提 |
| 2 | `fix(runtime): include backend error message in route failure logs` | P0-2 c、f | 否 | 消息需截断、脱敏 |
| 3 | `fix(frontend): log semantic request failure diagnostics at warn` | P0-2 d | 可选 | |
| 4 | `fix(sglang): surface prefill aborts in disaggregated handler` | P0-2 e | 否 | Python |
| 5 | `fix(llm): cancel paired prefill when decode fails before handoff` | F5 | 能力开关 | 改变了 handoff 的约定，建议先开 issue 讨论 |
| 6 | `fix(preprocessor): lower-bound context check for multimodal prompts` | F4（及 F5） | 否 | 精确计数单独再提一个 PR |
| 7 | `feat(preprocessor): sniff data-URI media and reject non-image payloads before streaming` | F2/F3 | 否或开关 | |
| 8 | `fix(llm): normalize integral JSON-schema bounds for guided decoding` | F1-a | 否 | 也可以同时向 xgrammar 提 |
| 9 | `feat(llm): relax or prevalidate response_format schemas for xgrammar` | F1-b / F1-c | 能力开关 | 同时向 SGLang 提议给预校验加策略开关 |
| 10 | `fix(frontend): OpenAI-compatible error body and context_length_exceeded message` | W2-0、E1 | 否 | |
| 11 | `feat(frontend): lenient request normalization on deserialize failure` | E3 | 无损部分默认开，有损部分 opt-in | |
| 12 | `feat(frontend): optional strict-schema fallback and dotted tool names` | E2 | opt-in | 与 1.6.0 的设计相反，需要先讨论 |
| 13 | `fix(frontend): tolerate malformed history tool-call arguments` | E4、E6 | 控制字符修复默认开，放行 opt-in | |
| 14 | `feat(frontend): TCP keepalive for the HTTP listener` | W5-1 | 环境变量 | |

**只在 fork 内做**：P0-1 配置、W5-2 配置、bootstrap 超时配置、F1-d 的 SGLang 插件临时方案、HEIC 插件（视上游意愿再定）。

---

## 10. 验收

1. **单元 / 集成测试**：各 PR 在上文列出的测试位置补充用例，例如 handoff.rs 的 prefill 取消、preprocessor 的长度下界、strict_schema / validate 的宽松路径、错误体序列化。
2. **线上复测**：用同一套 24h 分析方法复跑（脚本见 `out/scripts/`）。预期：

| 项 | 预期变化 |
|---|---|
| P0-1 | `vendor_error_frame` 类 400 基本消失，变为直接返回的 400 |
| P0-2 | `Stream closed unexpectedly` 能按 trace 查到，并带对端地址；F5 / E5 有具体原因 |
| W2 | E2 / E3 / E4 / E6 下降；E1 次数不变，但错误体带 `context_length_exceeded` |
| W3 | F1 接近 0（strict 时变为直接返回的 400） |
| W4 | F2 / F3 / F4 变为直接返回的 400，且有清晰消息；F5 不再出现 prefill 空等 300s |
| W5 | 配合链路修复后，A1 / B1 / C* 下降；C/B 类 120s 等待缩短 |

---

## 11. 每类一个例子（Dynamo 相关）

trace 链接格式：`https://obs-console.example/trace/<trace_id>`

| 子类 | 对应工作项 | trace_id | 时间（UTC） | 关键证据 |
|---|---|---|---|---|
| E1 | W2-0 / E1 | `103d176d989be5684cc803aabfb9af3a` | 11-16 03:47 | `{"type":"context_length","limit":1048576,"actual":1052243}`，请求体 3.2MB |
| E2 | E2 | `5ffaf8c4154daacee26c4675be432e7a` | 11-16 14:54 | `Invalid schema for function 'flag_issue': In context=#/required, must include property 'summary'` |
| E3 | E3 | `4d6f797d3ef4f81f20d15638ef7ef92b` | 11-16 08:12 | ``unknown variant `openrouter:web_search`, expected `function` `` |
| E4 | E4 | `61feff0aade76b4a38ac179a4ab9352a` | 11-17 02:08 | `messages[162].tool_calls[0].function.arguments must be a valid JSON object string: control character …` |
| E5 | P0-2 d / E5 | `dcdaa2125bbb933b1e52ed6558560180` | 11-16 23:05 | 只有 `{"message":"Invalid request"}` |
| E6 | E6 | `3899560a68096ccfd9f7cfe0846a7a3a` | 11-17 03:20 | `thinking must be a boolean or an object with type enabled/disabled/adaptive` |
| F1 | W3 | `378cf848844beaef98d508dd34b7f097` | 11-16 14:47 | `… correctsVersionId/anyOf/0 is not supported by xgrammar: constraints format, pattern cannot be enforced together` |
| F2 | F2/F3 嗅探 | `5416014900c8c7c281041c7283eee81b` | 11-16 08:59 | `Error while loading data data:audio/wav;base64` |
| F3 | F2/F3 嗅探 | `3c66c3930f52f6f510427efe8b60bfdc` | 11-16 16:47 | `Error while loading data data:image/png;base64`，`image format could not be determined` |
| F4 | F4 | `6a52b1c27c3c6a2526c79cdeebe979f9` | 11-16 18:29 | `The input (1234754 tokens) is longer than the model's context length (1048576 tokens)` |
| F5 | F5 | `113271f61ec960c549f070347c882543` | 11-16 09:06 | decode `rejected / backend_invalid_argument`（无消息）；prefill 300s 后 `Prefill bootstrap failed … KVPoll.Bootstrapping` |
| C1（链路，供 P0-2 / W5 对照） | P0-2 a、b | `2a6fc286ab0fa11175ca8762b2d3a952` | 11-16 04:36 | 响应头 473ms → `Stream closed unexpectedly` 1,700ms → prefill KV 报错 3,294ms；网关 120s 后才超时 |
