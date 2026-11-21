# ORCHESTRATION 2026 — Mechanisms for a Technically Superior H2 Supervisor

**Date:** 2026-11-13 (MSK) · **Author:** researcher subagent (deepseek-v4.1-flash) · **Owner:** tech-lead (H2)
**Scope:** durable execution, agent frameworks (handoffs/guardrails/tracing), protocols, observability/evals, sandboxing/budgets, supervision patterns.
**Method:** GitHub REST/GraphQL metadata for 45 repos (`gh api`, authenticated), primary docs via stealth-browser MCP (Chromium/nodriver, headless), in-repo docs fetched at `main` via `gh api ... Accept: raw`, plus file:line inspection of the local fork and H2 runtime.
**Access date for every web claim:** 2026-11-13. **Confidence legend:** `observed` (read in a primary source / command output), `inferred` (reasoned from observed), `reported-unverified` (vendor claim not independently checked), `unknown`.
**Evidence root:** `analysis/tech-lead/research/` (`gh_meta.tsv`, `raw/*.md`).
**Monitor ledger:** H2 `harness2_monitor` id **361** (`ORCHESTRATION_2026 deep research`).

---

## 0. What this document is (and is not)

This is a **mechanism catalog + adoption design**, not a popularity survey. Every entry answers one question: *what exactly should H2 copy, and where is the proof it works?* Star counts are recorded only as a maturity proxy and never used as an argument.

Two hard constraints shaped the selection:

1. **H2 runs on one Android device with root, no cluster.** Mechanisms that need a control plane, Postgres, or a VM fleet are still useful — but only their *protocol shape* (journal, memoized steps, task lifecycle, span schema) gets adopted, not their infrastructure.
2. **H2's supervisor already has an event stream and a ledger, but the stream is volatile.** `/api/event` is documented as *"Volatile by contract: a slow consumer overflows and fails the stream, and events during disconnection are missed"* (`packages/protocol/src/groups/event.ts:61-70`, observed). Therefore every completion mechanism adopted below must survive a missed event. That single local fact eliminates a whole class of "subscribe and hope" designs and drives §3.

---

## 1. Mechanism catalog (50 entries)

**Maturity scale:** M0 concept/spec only · M1 experimental · M2 production-usable · M3 production-proven · M4 de-facto standard. ⚠ = legal/abandonment risk for code reuse (mechanism may still be adopted).

### 1.1 Durable execution and resumability

| # | Project / paper | Version · date | License | Mechanism (what actually makes it durable) | Source (URL / file:line) | Mat. | What exactly to adopt in H2 |
|---|---|---|---|---|---|---|---|
| 1 | **Temporal server** | v1.32.2 · 2026-11-02 | MIT | Workflows as replayable state machines: durable **Event History**, deterministic replay, timers, signals, retries | https://github.com/temporalio/temporal (`README.md:23-24` observed: "durable execution platform … automatically handles intermittent failures, and retries failed operations") | M3 | Event-history shape for the ledger: append-only facts, derive state by folding. Never store mutable state as truth. |
| 2 | **Temporal Python SDK** | 1.36.0 · 2026-11-11 | MIT | Worker polling + workflow/activity split; activities are the only place side effects are allowed | https://github.com/temporalio/sdk-python | M3 | **Activity boundary rule** for H2: LLM turn = workflow step; tool/shell/file writes = activity that must be journaled before it is retried. |
| 3 | **Restate** | v1.8.1 · 2026-10-21 | **BSL 1.1** ⚠ | Journal of every side effect; replay skips completed steps; resilient service-to-service calls; awakeables (durable promises) | https://docs.restate.dev/foundations/key-concepts (observed: "Restate tracks every step … in a journal … replays the journal, skipping completed steps and resuming from exactly where it left off") | M2 | Journal-per-agent-phase with recorded *results*, not just intents — this is what makes relaunch cheap. |
| 4 | **Restate TS SDK** | v1.18.3 · 2026-11-04 | MIT | Same journal semantics in TypeScript; idempotency keys on ingress | https://github.com/restatedev/sdk-typescript | M2 | Idempotency key = `(ledger_id, phase, input_hash)` for every dispatch. |
| 5 | **DBOS Transact (TS)** | v5.5 · 2026-11-10 | MIT | Checkpoints workflows/steps **in Postgres**; no separate orchestrator; durable queues | https://docs.dbos.dev/architecture (observed: "annotating workflows and steps … checkpoints those workflows and steps to a Postgres database … recover each of your workflows from the last completed step") | M2 | **Ledger-in-SQLite as the orchestrator.** H2 already has SQLite; do not add a broker. Phase rows = step checkpoints. |
| 6 | **DBOS Transact (Python)** | 3.5.0 · 2026-11-10 | MIT | Same, Python runtime; recovery on process start | https://github.com/dbos-inc/dbos-transact-py | M2 | Startup recovery sweep: claimed-but-unreleased turns are resumed (see §3.7). |
| 7 | **Inngest** | v1.53.0 · 2026-11-12 | **SSPL-1.0** (Apache-2.0 future license) ⚠ | Step memoization: SDK hashes the **step ID**, stores `(hash, index, result)`; replay returns saved results and only re-runs the failing step | https://www.inngest.com/docs/durable-execution/durable-workflows (observed: "hashes the step ID … saves it in the run's state store … Later requests replay saved results (memoization)") | M3 | Phase memoization keyed by **declared step id**, not by call order. Critical for LLM agents whose step count varies. |
| 8 | **Trigger.dev** | v4.8.5 · 2026-11-12 | Apache-2.0 | **Checkpoint–Resume with CRIU**: while waiting on a subtask or `wait.for()`, the whole process (memory, CPU registers, open FDs) is checkpointed and restored later | https://trigger.dev/docs/how-it-works (observed: "uses CRIU … to create a checkpoint of the task's entire state, including memory, CPU registers, and open file descriptors") | M2 | Not portable to Android without root/criu — adopt the *semantic*: suspend/resume must preserve the agent's working set, not just its prompt. |
| 9 | **PydanticAI durable execution** | v2.68.0 · 2026-11-12 | MIT | Engine-agnostic durable agents: same agent code runs on Temporal, DBOS, Prefect, Restate, AWS Lambda, Absurd, pgtask, Airflow | https://pydantic.dev/docs/ai/capabilities/durable_execution/overview/ (observed list) | M2 | **Portability invariant**: H2 supervisor must not bake Temporal-specific words into packet/ledger schema. |
| 10 | **Mastra workflows** | @mastra/core 1.82.0 · 2026-11-04 | Apache-2.0 (excl. `ee/`) | Workflow graph with `suspend()` → snapshot of execution state → `resume` from a **step ID**; snapshots persist across deployments | https://mastra.ai/docs/workflows/suspend-and-resume (observed: "Suspension saves the current execution state as a snapshot. Later, resume from a specific step ID to restore the exact captured state … persist across deployments") | M2 | Resume by **phase id + snapshot ref**, with a schema version so old snapshots survive supervisor upgrades. |

### 1.2 Agent frameworks: handoffs, guardrails, tracing

| # | Project / paper | Version · date | License | Mechanism | Source | Mat. | Adopt |
|---|---|---|---|---|---|---|---|
| 11 | **OpenAI Agents SDK (Python)** | v0.24.0 · 2026-10-15 | MIT | Agents + **handoffs as tools** (`transfer_to_<agent>`), input/output/tool **guardrails with tripwires**, built-in tracing (traces/spans, processors, redaction) | https://github.com/openai/openai-agents-python `docs/handoffs.md:5-13` (observed: "Handoffs are represented as tools to the LLM … `transfer_to_refund_agent`"); `docs/tracing.md:3,15-20`; `docs/guardrails` page (observed: "Blocking execution guarantees that the expensive model does not start; with parallel execution, the expensive model may already have started") | M3 | (a) handoff = typed tool with acceptance criteria; (b) guardrail tripwire aborting before spend; (c) trace/span ids on every phase. |
| 12 | **OpenAI Agents SDK (JS)** | @openai/agents-extensions 0.22.0 · 2026-11-12 | MIT | Same primitives in TS, plus extensions package | https://github.com/openai/openai-agents-js | M2 | Cross-language parity of packet schema if H2 ever runs a JS supervisor. |
| 13 | **OpenAI Swarm** | no release · pushed 2026-04-15 | MIT | Two primitives only: `Agent` (instructions + tools) and **handoffs**; stateless between calls; explicitly "educational" | https://github.com/openai/swarm `README.md:76` (observed: "two primitive abstractions: `Agent`s and **handoffs**") | M0/M1 | Minimal handoff contract; do **not** copy the framework (archived-era, no durability). |
| 14 | **LangGraph** | 1.2.21 · 2026-11-12 | MIT | Checkpointer + `thread_id` persistence; interrupt/resume; time travel; "checkpoints growing unboundedly" is a documented failure mode | https://docs.langchain.com/oss/python/langgraph/persistence (observed: "Persistence lets LangGraph applications keep useful information beyond a single graph run … resume after an interruption, recover from a failure") | M3 | Thread/phase checkpoint + **explicit retention policy** (checkpoint GC), because unbounded checkpoints is the known failure. |
| 15 | **Agno / AgentOS** | v3.2.0 · 2026-11-12 | Apache-2.0 | Agents → teams → workflows; **AgentOS** runs them as durable services with auth, resume/replay, human approvals, scheduling | https://github.com/agno-agi/agno `README.md:17-20` (observed: "AgentOS handles authentication, role-based access, resume/replay, human approvals, scheduling, and durable execution") | M2 | The control-plane feature list is the checklist for v7: auth, resume/replay, approvals, scheduling. |
| 16 | **PydanticAI (core)** | v2.68.0 · 2026-11-12 | MIT | Typed agent I/O (Pydantic schemas), dependency injection, `pydantic-evals`, graph API | https://github.com/pydantic/pydantic-ai | M2 | Typed **packet and report schemas** validated at the boundary, not prose-only handoffs. |
| 17 | **Letta (server, legacy)** | 0.16.8 · 2026-05-14 | Apache-2.0 | Stateful agents with **memory blocks** that persist and are edited by the agent (MemGPT lineage) | https://github.com/letta-ai/letta `README.md` (observed: "Build stateful agents with memory that can learn and improve over time"; current source moved) | M2 (legacy) | Memory as an explicit, editable block with a size budget — not "append to context". |
| 18 | **Letta Code** | pushed 2026-11-13 | Apache-2.0 | Successor harness: stateful agents with memory, identity, experience; CLI + App Server + channels | https://github.com/letta-ai/letta-code `README.md:5-8` (observed) | M1/M2 | "Memory + identity + experience" triad for long-lived H2 agents; experience = distilled lessons (maps to `harness2_experience`). |
| 19 | **CrewAI + Flows** | 1.15.41 · 2026-11-12 | MIT | Event-driven Flows with shared state; `@persist` decorator persists flow state at class or method level, surviving restarts | https://docs.crewai.com/en/concepts/flows (observed: "The `@persist` decorator enables automatic state persistence … maintain flow state across restarts") | M2 | Per-phase state persistence flag; do not require a global checkpoint for cheap phases. |
| 20 | **AutoGen v0.4** | python-v0.7.5 · 2025-09-30 (repo pushed 2026-11-12) | MIT code / CC-BY-4.0 docs | **Actor model**: agents communicate only via messages through a runtime; standalone (single process) vs distributed runtime | https://github.com/microsoft/autogen `python/docs/src/user-guide/core-user-guide/core-concepts/architecture.md:3-22` (observed: "agents communicate via messages through the runtime, and the runtime manages …") | M2, maintenance lag | Message-passing boundary = supervisor↔agent packet; never shared mutable memory between agents. |
| 21 | **Microsoft Agent Framework** | python-1.25.0 · 2026-11-07 | MIT | Successor to AutoGen + Semantic Kernel: multi-agent workflows, .NET + Python, production focus | https://github.com/microsoft/agent-framework `README.md:12-14` (observed) | M2 | Migration signal: build on stable protocols, not on one framework's object model. |
| 22 | **Google ADK (Python)** | v2.14.1 · 2026-11-12 | Apache-2.0 | Code-first agents; evaluation and deployment built in; multi-agent composition | https://github.com/google/adk-python `README.md:16` (observed: "code-first Python framework for building, evaluating, and deploying … AI agents") | M2 | Eval/deploy as first-class lifecycle stages, not bolt-ons. |
| 23 | **Strands Agents SDK** | harness-typescript/v0.4.0 · 2026-11-12 | Apache-2.0 | Model-driven agent loop with tools/MCP; AWS-maintained | https://github.com/strands-agents/sdk-python | M2 | Model-driven tool selection is fine for leaves; supervisors still need explicit phase machines. |
| 24 | **Haystack** | v3.5.0 · 2026-11-10 | Apache-2.0 | Typed pipeline components with explicit connections; agents on top | https://github.com/deepset-ai/haystack | M3 | Pipeline-as-DAG for verification stages (build → test → review) instead of free-form agent chaining. |

### 1.3 Protocols

| # | Spec | Version · date | License | Mechanism | Source | Mat. | Adopt |
|---|---|---|---|---|---|---|---|
| 25 | **MCP** | revision 2025-11-25 (repo tag 2026-07-28) | MIT → Apache-2.0 transition | JSON-RPC 2.0, stateful connections, capability negotiation; servers expose resources/prompts/tools; hosts/clients/servers | https://modelcontextprotocol.io/specification/2025-11-25 (observed: "uses JSON-RPC 2.0 … Stateful connections … capability negotiation") | M4 | Tool capability negotiation before dispatch; never assume a tool exists. |
| 26 | **MCP Tasks (SEP-1686)** | 2025-11-25 revision | same | **Experimental tasks**: durable requests with polling and deferred result retrieval | https://modelcontextprotocol.io/specification/2025-11-25/changelog (observed: "Add experimental support for tasks to enable tracking durable requests with polling and deferred result retrieval (SEP-1686)") | M1 | This is the protocol-level analogue of H2's inbox/ack — validates the design; adopt polling+deferred retrieval semantics. |
| 27 | **A2A** | v1.0.1 · 2026-05-28 (spec examples carry `A2A-Version: 0.3`, observed — versioning ambiguity) | Apache-2.0 | Agent Card discovery; task lifecycle: SendMessage / SendStreamingMessage / GetTask / ListTasks / CancelTask / **SubscribeToTask** / push-notification configs; `contextId` + `taskId`; idempotency via `messageId` | https://a2a-protocol.org/latest/specification/ (observed TOC + "Send Message operations MAY be idempotent … utilize the messageId") | M2 | Canonical task state machine + `SubscribeToTask` as the model for H2's ack/subscribe; `contextId` = ledger id. |
| 28 | **OTel GenAI semantic conventions** | semconv 1.45.0 · 2026-11-02 (moved to `semantic-conventions-genai`, pushed 2026-11-13) | Apache-2.0 | Span ops `create_agent` / `invoke_agent` / `execute_tool`; span name `invoke_agent {gen_ai.agent.name}`; `gen_ai.agent.name` MUST be low cardinality; agent id/description attrs | https://github.com/open-telemetry/semantic-conventions-genai `docs/gen-ai/gen-ai-agent-spans.md:43-63` (observed) | M1→M2 | H2 span schema: one `invoke_agent` span per agent phase, `execute_tool` per tool, low-cardinality agent names (`researcher`, `implementer`, `reviewer`). |
| 29 | **OTel A2A + MCP conventions** | same repo · 2026-11-13 | Apache-2.0 | A2A spans model logical request/method/task lifecycle with `contextId`; MCP conventions for tool calls | https://github.com/open-telemetry/semantic-conventions-genai `docs/gen-ai/a2a.md:23-35` (observed) | M1 | Trace H2 handoffs as A2A-shaped spans (`taskId`, `contextId`) so traces survive a future protocol bridge. |

### 1.4 Observability and evals

| # | Project | Version · date | License | Mechanism | Source | Mat. | Adopt |
|---|---|---|---|---|---|---|---|
| 30 | **Langfuse** | v4.79.0 · 2026-11-12 | MIT (excl. `ee/`) | Traces capturing prompt/response/token usage/latency/tool steps; scores, datasets, dashboards, alerts | https://langfuse.com/docs/observability/overview (observed) | M3 | Trace fields = the minimum H2 must persist per phase even without an exporter. |
| 31 | **Arize Phoenix** | v21.1.0 · 2026-11-13 | **Elastic License 2.0** ⚠ | Tracing + evals + datasets/experiments on OpenTelemetry/OpenInference | https://arize.com/docs/phoenix (observed: "built on top of OpenTelemetry and is powered by OpenInference") | M2 | Eval-as-experiment model: same inputs, compare variants — for supervisor prompt/packet A/B. |
| 32 | **OpenInference** | @arizeai/openinference-genai 0.4.3 · 2026-11-12 | Apache-2.0 | Instrumentation + attribute conventions on OTel for LLM/agent spans | https://github.com/Arize-ai/openinference | M2 | Attribute naming source when OTel semconv is still "development" status. |
| 33 | **OpenLLMetry** | pushed 2026-11-09 | Apache-2.0 | One-line OTel instrumentation for LLM SDKs/frameworks | https://github.com/traceloop/openllmetry | M2 | Drop-in span emission for the supervisor without rewriting the agent loop. |
| 34 | **Inspect AI** | 0.3.289 · 2026-11-10 | MIT | `Task(dataset, solver, scorer)`, 200+ prebuilt evals, log viewer, and **sandboxing**: tool work (shell/Python) runs in provisioned containers while the model loop stays in-process | https://github.com/UKGovernmentBEIS/inspect_ai `docs/eval-sets.qmd:80-89`, `docs/sandboxing.qmd:8-24` (observed) | M2/M3 | Eval gate shape: dataset + scorer + threshold; sandbox for any tool that executes code. |
| 35 | **SWE-bench** | no release · repo pushed 2026-09-18 | MIT | Per-instance Docker harness; reproducible evaluation; splits: Lite 300, Verified 500, Multilingual 300/42 repos/9 languages, Multimodal 480 | https://www.swebench.com/ (observed) | M3 | Reproducibility rule: an eval claim is invalid without the exact harness + instance set + environment. |
| 36 | **SWE-agent** | v1.1.0 · 2025-05-22 (repo active) | MIT | Agent–computer interface (ACI): constrained file/edit/search commands instead of raw shell | https://github.com/SWE-agent/SWE-agent | M2 | Constrain implementer tool surface; fewer, typed tools = fewer irrecoverable actions. |
| 37 | **promptfoo** | 0.125.1 · 2026-10-25 | MIT | Declarative eval matrices, assertions, red-team suites, CI integration | https://github.com/promptfoo/promptfoo | M3 | Gate config as code (`assert:` per artifact), run in CI/closure. |
| 38 | **Ragas** | v0.4.3 · 2026-01-13 | Apache-2.0 | Reference-free RAG metrics (faithfulness, context precision/recall) | https://github.com/explodinggradients/ragas | M2 | Metric vocabulary for research agents; not central to supervisor. |
| 39 | **LangSmith SDK** | v0.14.9 · 2025-10-15 · **repo archived** | MIT | Tracing/eval SDK (client for hosted LangSmith) | https://github.com/langchain-ai/langsmith-sdk | M2, archived | Do not build on an archived SDK; keep OTel as the neutral export layer. |

### 1.5 Sandboxing and isolation

| # | Project | Version · date | License | Mechanism | Source | Mat. | Adopt |
|---|---|---|---|---|---|---|---|
| 40 | **E2B runtime** | 2026.30 · 2026-09-10 | Apache-2.0 | Firecracker-backed Linux VM per sandbox; **pause saves filesystem + memory**; paused sandboxes kept until killed; templates | https://docs.e2b.dev/ (observed: "A fast, secure Linux VM … pause and resume … Pausing a sandbox saves both its filesystem and its memory") | M2/M3 | Sandbox lifecycle = create → run → pause (snapshot) → resume → kill, with TTL and hard quota. |
| 41 | **Daytona** | v0.190.0 · 2026-06-23 · **archived 2026-07-24** | none declared | Sandboxes/snapshots/warm pools/isolation (docs still live at v0.231) | https://github.com/daytonaio/daytona `README.md:1-3` (observed: "**This repository is no longer maintained.**") | ⚠ abandoned | Do **not** adopt code; use only as a feature checklist (warm pools, snapshots, OTel collection). |
| 42 | **Firecracker** | v1.18.0 · 2026-11-09 | Apache-2.0 | Minimal VMM, one microVM per workload, jailer for host isolation, snapshot/restore | https://firecracker-microvm.github.io/ (observed: "purpose-built for creating and managing secure, multi-tenant container and function-based services"; "Apache 2.0 licensed") | M3 | On-device equivalent: proot/chroot + separate UID + read-only mounts; VM only when a real boundary is required. |
| 43 | **gVisor** | release-20261109.0 · 2026-11-11 | Apache-2.0 | User-space **application kernel** (Go) intercepting syscalls; OCI runtime `runsc`; not seccomp, not a VM | https://gvisor.dev/docs/ (observed: "an application kernel that implements a Linux-like interface … written in a memory-safe language (Go) and runs in userspace") | M3 | Syscall-level isolation is the honest boundary for "run untrusted code"; containers alone are not. |
| 44 | **OpenHands runtime** | v1.40.0 · 2026-11-12 | MIT | Agent platform with sandboxed runtime/workspace, event stream, and reproducible sessions | https://github.com/All-Hands-AI/OpenHands | M2/M3 | Session event stream + sandbox pairing (same shape as H2). |
| 45 | **Inspect sandboxing** (mechanism) | 0.3.289 · 2026-11-10 | MIT | Model/tool loop in main process; shell/Python tool execution delegated to provisioned containers via `sandbox()` | https://github.com/UKGovernmentBEIS/inspect_ai `docs/sandboxing.qmd:8-24` (observed) | M2 | Split "thinking" from "executing": supervisor stays unprivileged; only sandboxed workers get write/exec. |

### 1.6 Supervision patterns

| # | Source | Version · date | License | Mechanism | Source | Mat. | Adopt |
|---|---|---|---|---|---|---|---|
| 46 | **Anthropic — Building effective agents** | 2024-12 (page live 2026-11-13) | © Anthropic (web) | Workflow taxonomy: prompt chaining, routing, parallelization, **orchestrator-workers**, **evaluator-optimizer**; workflows vs agents distinction | https://www.anthropic.com/research/building-effective-agents (observed: "a central LLM dynamically breaks down tasks, delegates them to worker LLMs, and synthesizes their results") | M4 (concept) | v7 phase topology: orchestrator → workers; separate evaluator pass before close. |
| 47 | **Anthropic — multi-agent research system** | 2025-06 (page live 2026-11-13) | © Anthropic (web) | Lead agent + parallel subagents with **own context windows**; subagents compress before returning; lead steers based on intermediate findings | https://www.anthropic.com/research/multi-agent-research-system (observed: "orchestrator-worker pattern … subagents … operate in parallel … condensing the most important tokens") | M4 (concept) | Subagents must return **compressed evidence**, not transcripts; lead re-plans from intermediate results. |
| 48 | **GitHub Spec Kit / spec-driven development** | v1.4.0 · 2026-11-10; blog 2025-09-02 | MIT | Phased workflow `/specify` → `/plan` → `/tasks` with **validation checkpoints between phases** ("you don't move to the next one until the current task is fully validated") | https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/ (observed) | M2 | Packet template must be phase-gated; no implementation before an accepted plan artifact. |
| 49 | **HumanLayer** | pro-0.20.0 · 2025-12-23 | Apache-2.0 | Human steering of coding agents from one place: tasks, sessions, plans, reviews; approvals as a product surface | https://github.com/humanlayer/humanlayer `README.md:3` (observed) | M2 | Approval objects are durable and addressable (id, scope, expiry), not chat messages. |
| 50 | **Concordia** | v2.4.0 · 2026-03-06 | Apache-2.0 | Generative social simulation: entity–component agents + game master; multi-agent interaction under a shared world state | https://github.com/google-deepmind/concordia `README.md:3-19` (observed) | M1 | "Game master" pattern for supervisor: a single component owns world state and arbitrates agent actions. |

**Count: 50 entries** (≥40 required). All version/date/license values come from `analysis/tech-lead/research/gh_meta.tsv` (GitHub API, 2026-11-13) or the cited page; license caveats verified from the license files themselves where the SPDX API returned `NOASSERTION` (Restate BSL 1.1, Inngest SSPL 1.0, Mastra/Langfuse `ee/` split, Phoenix ELv2, AutoGen MIT code + CC-BY-4.0 docs).

---

## 2. Top 10 mechanisms to merge into the H2 supervisor and packet template

Each item = **mechanism → observed H2 gap → concrete design → evidence of correctness**. Numbering matches the user-requested list.

### 2.1 Event-driven completion (durable inbox + ack)

**Mechanism (Temporal event history, A2A `SubscribeToTask`, MCP tasks).** Completion is a *fact to reconcile*, not a notification to hope for.

**Observed H2 gap.** `/api/event` is explicitly lossy: *"Volatile by contract: a slow consumer overflows and fails the stream, and events during disconnection are missed"* (`packages/protocol/src/groups/event.ts:61-70`). Despite this, v6 supervision treats the SSE subscription as the completion channel. Any supervisor restart, stream overflow, or serve restart during a turn loses the completion event. The ack directory exists but is keyed by `session:timestamp` strings (`~/.harness2/supervisor/ack/index.json`, observed) with no durable event id and no replay path.

**Design.**
- `supervisor_inbox(id INTEGER PK AUTOINCREMENT, session_id TEXT, event_type TEXT, execution_started_at REAL, received_at REAL, payload_json TEXT, dedupe_key TEXT UNIQUE)`.
- `dedupe_key = sha256(session_id || event_type || execution_started_at)` — SSE redelivery after reconnect cannot double-count.
- Subscriber = one long-lived process: connect `GET /api/event?types=session.execution.started|succeeded|failed|interrupted`, write row, flush, ack **in the ledger** within one sweep (≤60 s).
- **Gap detection:** on every reconnect, compare `session.execution.started` rows without a terminal row against the session message DB (`session` / message tables). A started-without-terminal older than the stall threshold is escalated as `unrecorded_completion`, not ignored.
- **Polling fallback:** `GET /api/session/active` (`packages/protocol/src/groups/session.ts:220`) every 15 s; active set minus inbox-derived running set = "running but invisible" → force a ledger checkpoint probe.

**Verification.** Kill -TERM the subscriber mid-turn; after restart the terminal event must still be reconciled from either the inbox or the message DB, with `completion_unrecorded=0` at the next sweep.

### 2.2 Heartbeat and stall detection as first-class ledger columns

**Mechanism (Temporal heartbeat/timeout, Trigger.dev checkpoint age, A2A task updates).**

**Observed H2 gap.** `subagent_ledger` DDL has **no heartbeat column at all** — `h2oc.rs:834` defines `task_ref, session_id, parent_session_id, role, agent, description, owner_paths, read_only_paths, expected_evidence, background, state, last_status, note, h2_ids_json, source, created_at, updated_at, meta_json`. `rg heartbeat src/rust-hooks/src/h2oc.rs` returns nothing (observed). Yet `AGENTS.md:251-262` claims a heartbeat gate. The gate's *contract* exists; the *substrate* does not. This is exactly the "false durability" class in §4.

**Design.**
- Add columns (additive, nullable): `phase TEXT`, `heartbeat_at REAL`, `phase_started_at REAL`, `deadline_at REAL`, `stall_timeout_s INTEGER DEFAULT 90`, `attempt INTEGER DEFAULT 0`, `max_attempts INTEGER DEFAULT 2`, `budget_json TEXT`.
- Index: `CREATE INDEX idx_ledger_liveness ON subagent_ledger(state, heartbeat_at)`.
- Agent contract: heartbeat **at every phase change and at least every 90 s**, carrying `{phase, progress, last_tool, tokens_used, artifacts[]}`. A subagent may not declare itself healthy — liveness is computed by the supervisor (`now - heartbeat_at <= stall_timeout_s`).
- Sweep every 60 s: `warn` at 0.66×timeout, `stall` at 1.0×, `dead` at 2×. Each transition writes a checkpoint row and an inbox event.

**Verification.** Freeze an agent (SIGSTOP); ledger must show `warn → stall → dead` with timestamps, and the relaunch packet must contain the last heartbeat payload.

### 2.3 Durable resume with a continuation packet and memoized phases

**Mechanism (Restate replay, Inngest step memoization, Mastra snapshot, LangGraph checkpointer, Trigger.dev checkpoint–resume).** Resume must skip completed work by **identity**, not by re-running and hoping.

**Observed H2 gap.** Relaunch exists as a concept (`~/.harness2/supervisor/relaunch/`, `queue/` files such as `agent-anim-merge-325-*.json` with `{session, ledger, task_ref, state, idle_age_s, last_note, detected}` — observed). There is no phase-level checkpoint table, so a relaunched agent re-reads the same files, re-runs the same commands, and can duplicate side effects.

**Design.**
- `agent_phase(ledger_id, phase, step_key, status, input_hash, artifact_ref, evidence_json, started_at, finished_at, PRIMARY KEY(ledger_id, phase, step_key))`.
- `step_key` is **declared in the packet** (e.g. `p1.read-spec`, `p2.apply-edit-auth`, `p3.run-pytest`), never inferred from call order — Inngest's hashed step id is the proof that order-based keys break when an agent retries differently.
- On relaunch: supervisor emits the continuation packet with `completed_phases[]` + artifact hashes; worker tool wrapper short-circuits any `step_key` already `done` and returns the recorded result (journal semantics).
- Snapshot = `{packet_version, completed_phases, open_questions, last_evidence, next_action}`; store as JSON next to the ledger row so a supervisor upgrade can migrate it (`schema_version`).

**Verification.** Kill an implementer after `p2` and relaunch; `p1/p2` must not re-execute (assert zero new file mtimes / zero duplicate tool spans), and the diff must continue from `p3`.

### 2.4 Steering cadence (bounded, phase-gated, logged)

**Mechanism (Temporal signals, A2A multi-turn `contextId`, Anthropic lead-agent re-planning).** Steering is a durable input, not a chat interruption.

**Observed H2 gap.** `AGENTS.md:272-278` requires "actively steer every running agent at least once per phase", but there is no steer record with id/phase and no de-duplication; a repeated steer can arrive twice after a reconnect.

**Design.**
- `steer(id TEXT PK, ledger_id, phase, kind, body_ref, created_at, delivered_at, applied_at, supersedes)`.
- Delivery via the session inbox primitive that already exists as a durable event (`session.inbox.enqueued` / `session.inbox.delivered`, `packages/schema/src/session-event.ts:213-235`) rather than a raw prompt injection.
- Cadence rule: one steer per phase **minimum**, one per stall event, never two contradictory steers without `supersedes`; a steer that arrives after phase completion is `applied_at=NULL` and reported as `steer_late`.
- Coherence check before send: OWNER_PATHS disjointness, no reversal of an invariant, no new scope.

**Verification.** Send the same steer twice; exactly one `applied` row; late steer shows `steer_late=1` in the close report.

### 2.5 Eval gates (evidence, not vibes)

**Mechanism (Inspect AI Task/dataset/scorer, SWE-bench harness reproducibility, promptfoo assertions).**

**Observed H2 gap.** Close requires "rc/artifact/screenshot + h2 IDs" (`AGENTS.md:260`), which is a *presence* check, not a *quality* gate. There is no scorer, no threshold, and no per-instance reproduction for code work.

**Design.**
- Gate types: `static` (lint/typecheck), `unit` (named test command + expected rc), `behavioral` (fixture input → expected artifact/schema), `review` (independent reviewer agent + rubric), `budget` (tokens/tool-calls within cap).
- Each gate declares `{id, command_or_scorer, threshold, artifact_path, timeout_s}` in the packet; the supervisor executes or verifies it and stores `evidence_json` (stdout tail + rc + hash).
- A close with any `must` gate absent = `partial`, never `done`. SWE-bench's rule generalizes: a result is only valid with the exact harness + instance set + environment recorded.
- Reviewer agent is not the implementer (Anthropic evaluator-optimizer); a self-review cannot satisfy a review gate.

**Verification.** Deliberately break one test; close must return `partial` with the failing gate id and rc, and the ledger must not transition to `closed`.

### 2.6 Tracing and budget per phase

**Mechanism (OTel GenAI spans, Agents SDK tracing, Langfuse trace fields, LangGraph unbounded-checkpoint warning).**

**Observed H2 gap.** No span model and no budget columns; cost blowups are detected only after the fact via token dumps.

**Design.**
- Span schema: `invoke_agent {gen_ai.agent.name}` per phase, `execute_tool {gen_ai.tool.name}` per tool, attributes `gen_ai.operation.name`, `gen_ai.agent.name` (low cardinality), `h2.ledger_id`, `h2.phase`, `h2.attempt`, `h2.budget.remaining`.
- Export: local JSONL always (works offline); OTLP optional. Never export raw prompts without redaction — Agents SDK has explicit "Sensitive data" / "Redaction before export" sections, which exist because default tracing captures prompts.
- Budget: `{wall_clock_s, tokens_in, tokens_out, tool_calls, disk_mb}` per agent and per phase; on 80% → steer to narrow scope; 100% → phase transition `budget_exhausted` + report, no silent continuation.
- Retention: trace rows per ledger with TTL; checkpoint GC (LangGraph's "checkpoints growing unboundedly" is a documented failure, not a hypothetical).

**Verification.** Run a phase with `tool_calls=3`; span count == 3 and budget row shows `remaining=0` → transition.

### 2.7 Sandbox per agent (isolation is a spectrum, not a boolean)

**Mechanism (E2B microVM pause/resume, Firecracker, gVisor, Inspect sandboxing).**

**Observed H2 gap.** Agents run with full user privileges in `$HOME`; owner paths are enforced by instruction, not by the OS. One `rm -rf` outside OWNER_PATHS is unrecoverable.

**Design.**
- Tier 0 (default): separate working dir + `OWNER_PATHS` allowlist enforced by a tool wrapper (path resolve + prefix check), read-only mounts for the rest.
- Tier 1: separate Unix UID (`su` available) + read-only bind of the repo + writable scratch; network egress limited.
- Tier 2 (untrusted code / donor builds): proot/chroot or microVM-class boundary; no host FS; explicit CPU/mem/disk/time quota; snapshot before run (E2B pause semantics).
- "Sandbox" is not claimed unless the boundary is enforced by the kernel/UID — gVisor's own docs make the distinction between seccomp, containers, and an application kernel.
- Approval object (HumanLayer pattern) for any Tier-2 action and any destructive op.

**Verification.** Attempt a write outside OWNER_PATHS from a Tier-1 worker: EACCES with audit log line; repo hash unchanged.

### 2.8 Handoff protocol (typed, tool-shaped, with acceptance criteria)

**Mechanism (Agents SDK handoffs-as-tools, Swarm primitives, A2A task lifecycle, AutoGen message boundary).**

**Observed H2 gap.** Handoffs are Markdown packets (`AGENT_PACKET_TEMPLATE.md`) with no schema validation, no acceptance criteria field, and no return contract; a subagent can "complete" without producing the required artifact list.

**Design.**
- `HandoffPacket` schema (versioned): `mission`, `parity_row`, `invariants[]`, `owner_paths[]`, `read_only_paths[]`, `phases[{id, goal, steps[{key, verify}]}]`, `acceptance[{id, artifact, check}]`, `budget`, `handoff_back{required_evidence[], format}`, `intervention{heartbeat_s, steer_channel}`.
- Handoff is a tool call with a name (`transfer_to_implementer`) and a **return contract**; the supervisor validates the packet before dispatch (missing field → reject, not "best effort").
- Return message is a `Report` schema: `result`, `evidence[]` (rc/artifact/hash), `phases_done[]`, `open_questions[]`, `next_action`.
- A2A mapping: `contextId = ledger_id`, `taskId = phase id`, `messageId = steer id`; this makes a future protocol bridge mechanical rather than aspirational.

**Verification.** Dispatch a packet with `acceptance` missing → rejected with schema error; valid packet → accepted and ledger row created.

### 2.9 Memory and dream consolidation

**Mechanism (Letta memory blocks / Letta Code experience, H2 `harness2_experience` + `h2oc dream`).**

**Observed H2 gap.** Experience rows accumulate but are not systematically validated against captures; memory injection is "top-N", which is an unbounded-context failure waiting to happen.

**Design.**
- Three tiers: (a) **working memory** = packet + continuation snapshot (bounded by schema); (b) **episodic** = ledger + trace + artifacts (addressable, not injected); (c) **semantic** = distilled experiences (small, deduped, evidence-linked).
- Dream pass: validate experience against capture evidence; merge duplicates; mark contradictions; promote only evidence-backed lessons; write `wiki/core`; cap injected semantic memory by token budget, not row count.
- Every promoted lesson carries `source_capture_id` + `confidence`; a lesson without evidence is `unverified` and never injected as a rule.

**Verification.** Inject two contradictory experiences; dream must mark one `contradiction` and refuse promotion until evidence resolves it.

### 2.10 Spec packets (phase-gated, machine-checkable)

**Mechanism (GitHub Spec Kit: specify → plan → tasks with validation checkpoints).**

**Observed H2 gap.** The template exists (AGENTS.md:272) but is prose; nothing prevents skipping plan validation or starting implementation from a one-line ask.

**Design.**
- Packet lifecycle states: `draft → spec_ready → plan_ready → tasks_ready → dispatched`. Transitions require artifacts (`spec.md`, `plan.md`, `tasks.json`) and a gate result.
- `spec.md` = goal, non-goals, invariants, acceptance criteria, risks. `plan.md` = phases with verification commands. `tasks.json` = step keys + owner paths + budgets (machine-checkable).
- Supervisor refuses dispatch unless `tasks_ready` and schema-valid; this is the packet-level analogue of Spec Kit's "don't move to the next phase until the current task is fully validated".

**Verification.** Attempt dispatch from `spec_ready` → `packet_incomplete` with missing artifact names.

---

## 3. Supervisor v7 — concrete design

### 3.1 Goals and non-goals

**Goals:** (1) no completion is ever lost; (2) no agent silently stalls; (3) relaunch never re-does completed work; (4) every close is evidence-backed; (5) cost is bounded per agent; (6) the supervisor survives its own restart.

**Non-goals:** distributed consensus, multi-host scheduling, exactly-once execution. H2 runs on one device; the claim is **at-least-once with idempotent phases**, and the docs must say so (see §4.1).

### 3.2 Architecture

```
                ┌──────────────────────── supervisor v7 ────────────────────────┐
 /api/event ───▶│ ingest (SSE, filters)  ──▶ inbox (SQLite, UNIQUE dedupe_key)  │
                │      │                          │                             │
                │      └── gap detector ◀─────────┘                             │
 /api/session/active (poll 15s) ──▶ liveness oracle ──▶ sweep (60s)            │
                │                                        │                      │
                │   ledger(subagent_ledger + phase cols) ◀┘                     │
                │        │            │            │                           │
                │   phase FSM     budget/trace   eval gates                     │
                │        │            │            │                           │
                │        └──────▶ relaunch (bounded) ──▶ continuation packet    │
                └───────────────────────────────┬───────────────────────────────┘
                                                │
                                   opencode2 run --session <ses> (v2 API)
```

### 3.3 Data model (additive, no destructive migration)

```sql
-- liveness + budget on the existing ledger
ALTER TABLE subagent_ledger ADD COLUMN phase TEXT;
ALTER TABLE subagent_ledger ADD COLUMN heartbeat_at REAL;
ALTER TABLE subagent_ledger ADD COLUMN phase_started_at REAL;
ALTER TABLE subagent_ledger ADD COLUMN deadline_at REAL;
ALTER TABLE subagent_ledger ADD COLUMN stall_timeout_s INTEGER DEFAULT 90;
ALTER TABLE subagent_ledger ADD COLUMN attempt INTEGER DEFAULT 0;
ALTER TABLE subagent_ledger ADD COLUMN max_attempts INTEGER DEFAULT 2;
ALTER TABLE subagent_ledger ADD COLUMN budget_json TEXT;

CREATE INDEX IF NOT EXISTS idx_ledger_liveness ON subagent_ledger(state, heartbeat_at);

CREATE TABLE IF NOT EXISTS supervisor_inbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedupe_key TEXT NOT NULL UNIQUE,
  session_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  execution_started_at REAL,
  received_at REAL NOT NULL,
  payload_json TEXT
);

CREATE TABLE IF NOT EXISTS supervisor_ack (
  dedupe_key TEXT PRIMARY KEY,
  ledger_id INTEGER,
  acked_at REAL,
  acked_by TEXT,
  evidence_json TEXT
);

CREATE TABLE IF NOT EXISTS agent_phase (
  ledger_id INTEGER NOT NULL,
  phase TEXT NOT NULL,
  step_key TEXT NOT NULL,
  status TEXT NOT NULL,           -- pending|running|done|failed|skipped
  input_hash TEXT,
  artifact_ref TEXT,
  evidence_json TEXT,
  started_at REAL,
  finished_at REAL,
  PRIMARY KEY (ledger_id, phase, step_key)
);

CREATE TABLE IF NOT EXISTS steer (
  id TEXT PRIMARY KEY,
  ledger_id INTEGER NOT NULL,
  phase TEXT,
  kind TEXT,                      -- hint|correction|priority|evidence
  body_ref TEXT,
  created_at REAL, delivered_at REAL, applied_at REAL,
  supersedes TEXT
);
```

**Why SQLite and not a broker:** DBOS demonstrates the whole durability story on Postgres with no separate orchestrator; the same argument holds for SQLite at H2's scale, and the ledger already lives there.

### 3.4 Per-agent phase state machine

```
registered → dispatched → running(phase)
                 │             ├─ heartbeat_ok      → running(phase)
                 │             ├─ phase_done        → running(next) | awaiting_evidence
                 │             ├─ stall (>timeout)  → stalled
                 │             ├─ budget_exhausted  → budget_stop
                 │             └─ failed            → failed
                 ├─ awaiting_evidence → close_gate → closed | partial
                 ├─ stalled → relaunch(attempt<max) → dispatched(continuation)
                 └─ failed  → relaunch(attempt<max) → dispatched(continuation) | blocked
terminal: closed | partial | blocked            (all require evidence_json + h2 IDs)
```

Invariants:
- No `closed` without ack row + eval gate evidence.
- `blocked` requires an external-boundary note with exact next step and a named owner (AGENTS.md no-open-tails).
- `attempt` increments on every relaunch; `attempt == max_attempts` forces `blocked` (no infinite retry storm).

### 3.5 Event ingest and reconciliation

1. SSE connect with type filter; each event → `inbox` insert (`INSERT OR IGNORE` on `dedupe_key`).
2. Terminal event for a session with no ledger row → `orphan_completion` (the dispatch lost the register call); supervisor reconstructs the ledger from the last dispatch packet on disk.
3. On reconnect: `SELECT` sessions with `started` and no terminal; query session messages for a terminal part; if found, synthesize the terminal row with `source='reconciled'`.
4. Every sweep writes a single line: `sweep ts=… ledger=… running=… stalled=… completion_unrecorded=… orphan=…` — the log is the audit trail, not the state.

### 3.6 Sweep algorithm (60 s)

```
for row in ledger where state in (dispatched, running, awaiting_evidence):
    age = now - coalesce(heartbeat_at, updated_at)
    if age > 2*stall_timeout_s: state=dead
    elif age > stall_timeout_s: state=stalled
    elif age > 0.66*stall_timeout_s: warn(checkpoint only)
    if budget_exceeded(row): state=budget_stop
for stalled|dead|failed where attempt < max_attempts:
    build continuation packet from agent_phase(done) + last evidence
    relaunch via v2 API with --session <same session>
for completed sessions with no ack:
    ack + close(ledger) or close(partial) with missing list
```

### 3.7 Crash recovery of the supervisor itself

- On start: load ledger rows in non-terminal states; load inbox unacked; call `/api/session/active`.
- Three-way reconciliation: ledger says running, session says idle → completion missing (reconcile from messages); ledger says idle, session says running → relaunch-less adoption (re-attach); both running but no heartbeat → stalled.
- Never kill a session to "clean up"; only reconcile state.
- The session-side recovery already exists for turns: `packages/core/src/session/execution/restart.ts:38` "Resumes Sessions whose execution claim was never released" and `store.ts:69-74` (observed) — v7 must *observe* this, not duplicate it.

### 3.8 Relaunch / continuation packet

```json
{
  "schema_version": 1,
  "ledger_id": 361,
  "attempt": 1,
  "mission": "…unchanged…",
  "completed_phases": [
    {"phase": "p1", "step_keys": ["p1.read-spec"], "artifacts": [{"path": "…", "sha256": "…"}]}
  ],
  "do_not_repeat": ["p1.read-spec"],
  "open_questions": ["…"],
  "next_action": {"phase": "p2", "step_key": "p2.apply-edit", "verify": "pytest -q tests/x.py"},
  "evidence_so_far": [{"cmd": "…", "rc": 0, "tail": "…"}],
  "budget_remaining": {"tool_calls": 40, "tokens": 120000, "wall_clock_s": 1800},
  "acceptance": [{"id": "a1", "artifact": "…", "check": "…"}]
}
```

Rules: packet is validated against schema; `do_not_repeat` is enforced by the tool wrapper (step short-circuit); every relaunch increments `attempt` and is logged with the stall evidence that caused it.

### 3.9 Steering

- Steer cadence: at least one per phase; forced on `warn`; never after `closed`.
- Delivery through the durable inbox events; `applied_at` set by the agent's checkpoint, not by the sender.
- Contradiction check before send; `supersedes` required when reversing a prior steer.

### 3.10 Tracing and budget

- One `invoke_agent` span per phase + one `execute_tool` span per tool call; attributes per OTel GenAI (`gen_ai.operation.name`, low-cardinality `gen_ai.agent.name`) plus H2 ids.
- Redaction before export (prompts may contain paths/secrets); local JSONL is the source of truth, OTLP optional.
- Budgets enforced at the tool boundary, not by asking the model to be frugal.

### 3.11 Eval gates

- `must` gates: schema valid packet, static checks, named tests, acceptance artifacts present + hash, reviewer verdict for code changes.
- Gate results stored in `agent_phase.evidence_json` and copied to the close report.
- A `partial` close lists exactly which gate is missing and why; `blocked` additionally needs an external-boundary justification.

### 3.12 Rollout plan (no code changes in this task)

1. **Phase A (schema + ingest):** migration + inbox/ack + sweep log line. Success: `completion_unrecorded=0` across 10 dispatches including one forced subscriber kill.
2. **Phase B (heartbeat + FSM):** heartbeat columns, warn/stall/dead transitions, one relaunch with continuation packet. Success: freeze test produces `stalled → relaunch → closed` with no duplicate phase work.
3. **Phase C (gates + budget + trace):** eval gates and budget stops. Success: broken test → `partial`; budget cap → `budget_stop`.
4. **Phase D (sandbox tiers + steering ledger).** Success: out-of-owner-path write denied with audit line; duplicate steer applied once.

---

## 4. Anti-patterns observed

### 4.1 Fake durability (claiming resume without a journal)

Temporal's model only works because side effects are recorded and replay is deterministic; Restate records the *result* of each step; Inngest keys memoization by hashed step id. A supervisor that "relaunches the session" without phase records re-executes side effects and calls it recovery. **H2 currently has relaunch artifacts but no phase table** (`~/.harness2/supervisor/relaunch/` observed; no `agent_phase` equivalent in `h2oc.rs`). Fix = §2.3, not more retries.

### 4.2 Treating a volatile stream as a queue

Our own `/api/event` says events during disconnection are missed (`event.ts:69`). Any design that makes SSE the *only* completion channel is lossy by construction. MCP had to add experimental **tasks** with polling/deferred retrieval for exactly this reason (SEP-1686, observed). Fix = durable inbox + gap detection + `session.active` polling.

### 4.3 Retry storms and duplicate work

Retries without attempt accounting, idempotency keys, or backoff turn one failure into N side effects — the reason Temporal/Restate/DBOS all put idempotency at the boundary. Observed H2: `max_attempts` does not exist in the ledger schema; `attempt` is not a column. Fix = bounded attempts + `(ledger_id, phase, step_key)` uniqueness.

### 4.4 Unbounded context and unbounded checkpoints

LangGraph's own troubleshooting section lists "Checkpoints growing unboundedly" (observed heading). Anthropic's multi-agent system relies on subagents compressing into their own context windows before returning. H2's counter-pattern would be: inject all experiences, keep all transcripts, never GC. Fix = three-tier memory + token-capped injection + trace/checkpoint retention.

### 4.5 Thin packets and dispatch-then-wait

A one-line prompt has no acceptance criteria, no owner paths, no verification, so "done" becomes unfalsifiable. Spec Kit exists to force phase validation; H2's own `AGENTS.md:272` already forbids thin dispatches, but the template is prose and unvalidated. Fix = schema-validated `tasks_ready` gate (§2.10) and a return `Report` schema.

### 4.6 Notification-only supervision

Activity (files changing, processes alive) proves work is happening; only a completion signal + ledger evidence proves an agent finished. The v6 queue files record `state=completed, idle_age_s=396` (observed `~/.harness2/supervisor/queue/agent-anim-merge-325-*.json`) — a *detection*, not a reconciliation. Fix = ack within one sweep, else `completion_unrecorded` escalation.

### 4.7 Framework and license lock-in

Observed in this research: Daytona's core repo is **archived and has no license** (`README.md:1-3`); Restate is BSL 1.1; Inngest is SSPL 1.0; Phoenix is ELv2; Mastra and Langfuse carve out `ee/` directories; AutoGen's last release is 2025-09-30 while Microsoft Agent Framework is the active successor; Letta moved its source to `letta-code`; the LangSmith SDK repo is archived. Copying framework code into H2 imports that risk. Fix = adopt **mechanisms and schemas**, keep the supervisor's own thin implementation, and record license/abandonment status per donor (this table is that record).

### 4.8 Sandbox theater

Running untrusted code in the same UID/HOME and calling it sandboxed is not isolation. gVisor's docs explicitly separate seccomp filters, container wrappers, and an application kernel; Firecracker exists because VM boundaries differ from container boundaries; Inspect sandboxes only the tool execution, not the model loop. Fix = tiered isolation with an explicit claim per tier (§2.7).

### 4.9 Tracing without redaction

Agents SDK's tracing docs ship "Sensitive data" and "Redaction before export" sections; that need exists because default traces capture prompts verbatim. H2 traces will contain paths, tokens, and possibly secrets. Fix = redact before any export, keep raw traces local and short-lived.

### 4.10 Human-in-the-loop as an afterthought

If approval is a chat message, resume-after-approval has no durable object. LangGraph interrupts, Agno approvals, Mastra suspend, and HumanLayer all model approval as a first-class object. Fix = `approval(id, scope, expires_at, decision, evidence)` rows tied to a phase.

---

## 5. Adoption priority

| Priority | Item | Why now | Cost | Risk if skipped |
|---|---|---|---|---|
| P0 | Durable inbox + gap detection (§2.1) | SSE is lossy by contract; v6 loses completions | low (one table + subscriber) | phantom "running" agents, lost closes |
| P0 | Heartbeat/attempt/budget columns (§2.2) | Gate claims a substrate that does not exist | low (additive migration) | stall detection is fiction |
| P0 | Phase memoization + continuation packet (§2.3) | Relaunch currently repeats work | medium | duplicate edits, wasted budget |
| P1 | Schema-validated packets + return report (§2.8/2.10) | Thin packets are the root of unverifiable work | medium | "done" without artifacts |
| P1 | Eval gates (§2.5) | Presence checks ≠ quality gates | medium | false closes |
| P1 | Budget + trace schema (§2.6) | Cost blowups detected late | medium | runaway spend |
| P2 | Steering ledger (§2.4) | Cadence exists on paper only | low | contradictory/duplicate steers |
| P2 | Sandbox tiers (§2.7) | Instruction-level path safety | medium | destructive escape |
| P2 | Memory/dream caps (§2.9) | Unbounded injection | medium | context rot |

---

## 6. Source ledger (access date 2026-11-13, method)

| Source | URL | Method | Completeness |
|---|---|---|---|
| Temporal README | https://github.com/temporalio/temporal | `gh api` raw README | full file |
| Restate key concepts | https://docs.restate.dev/foundations/key-concepts | stealth-browser (Chromium headless) | excerpt around "Durable Execution" |
| DBOS architecture | https://docs.dbos.dev/architecture | stealth-browser | excerpt around architecture |
| Inngest durable workflows | https://www.inngest.com/docs/durable-execution/durable-workflows | stealth-browser | excerpt around memoization |
| Trigger.dev how it works | https://trigger.dev/docs/how-it-works | stealth-browser | excerpt around CRIU checkpoint |
| Mastra suspend/resume | https://mastra.ai/docs/workflows/suspend-and-resume | stealth-browser | excerpt around snapshots |
| PydanticAI durable execution | https://pydantic.dev/docs/ai/capabilities/durable_execution/overview/ | stealth-browser (redirect from ai.pydantic.dev) | intro |
| CrewAI Flows | https://docs.crewai.com/en/concepts/flows | stealth-browser | excerpt around `@persist` |
| OpenAI Agents SDK handoffs | https://github.com/openai/openai-agents-python `docs/handoffs.md` | `gh api` raw | full file |
| OpenAI Agents SDK tracing | `docs/tracing.md` | `gh api` raw | full file |
| OpenAI Agents SDK guardrails | https://openai.github.io/openai-agents-python/guardrails/ | stealth-browser | excerpt |
| LangGraph persistence | https://docs.langchain.com/oss/python/langgraph/persistence | stealth-browser (redirect) | excerpt + troubleshooting headings |
| AutoGen architecture | https://github.com/microsoft/autogen `python/docs/.../architecture.md` | `gh api` raw | full file |
| MCP spec | https://modelcontextprotocol.io/specification/2025-11-25 | stealth-browser | overview |
| MCP key changes (tasks) | https://modelcontextprotocol.io/specification/2025-11-25/changelog | stealth-browser | changelog excerpt |
| A2A spec | https://a2a-protocol.org/latest/specification/ | stealth-browser | TOC + semantics excerpt |
| OTel GenAI agent spans | https://github.com/open-telemetry/semantic-conventions-genai `docs/gen-ai/gen-ai-agent-spans.md` | `gh api` raw | full file (51 KB) |
| OTel A2A conventions | `docs/gen-ai/a2a.md` | `gh api` raw | full file |
| OTel semconv release | https://github.com/open-telemetry/semantic-conventions | `gh api` | metadata |
| Langfuse observability | https://langfuse.com/docs/observability/overview | stealth-browser | excerpt |
| Arize Phoenix | https://arize.com/docs/phoenix | stealth-browser | excerpt |
| Inspect AI eval sets | `docs/eval-sets.qmd` | `gh api` raw | full file |
| Inspect AI sandboxing | `docs/sandboxing.qmd` | `gh api` raw | full file |
| SWE-bench leaderboard | https://www.swebench.com/ | stealth-browser | splits + news |
| E2B docs | https://docs.e2b.dev/ | stealth-browser | intro + persistence |
| Daytona repo | https://github.com/daytonaio/daytona | `gh api` raw README | full file + archive flag |
| Firecracker site | https://firecracker-microvm.github.io/ | stealth-browser | intro |
| gVisor docs | https://gvisor.dev/docs/ | stealth-browser | intro |
| Anthropic building effective agents | https://www.anthropic.com/research/building-effective-agents | stealth-browser (canonical redirect) | orchestrator/optimizer excerpts |
| Anthropic multi-agent research | https://www.anthropic.com/research/multi-agent-research-system | stealth-browser (canonical redirect) | architecture excerpt |
| GitHub spec-driven blog | https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/ | stealth-browser | phase excerpt |
| Spec Kit repo | https://github.com/github/spec-kit | `gh api` | metadata + README |
| Local SSE contract | `packages/protocol/src/groups/event.ts:61-70` | local file read | exact |
| Local execution events | `packages/schema/src/session-event.ts:244-263` | local file read | exact |
| Local inbox events | `packages/schema/src/session-event.ts:213-235` | local file read | exact |
| Local ledger DDL | `src/rust-hooks/src/h2oc.rs:834` | rg | exact |
| Local session recovery | `packages/core/src/session/execution/restart.ts:38`, `store.ts:69-74` | rg | exact |
| Local `/api/session/active` | `packages/protocol/src/groups/session.ts:220` | rg | exact |
| Supervisor artifacts | `~/.harness2/supervisor/{inbox,queue,ack,relaunch}`, `ack/index.json` | file read | samples |
| Repo metadata (45 repos) | `analysis/tech-lead/research/gh_meta.tsv` | `gh api` REST | complete |

**Limitations (honest):**
- Temporal docs are a JS-rendered SPA; `docs.temporal.io/workflows` returned an empty body to the headless extractor on 3 attempts. Temporal claims therefore rest on the repo README (observed) plus the docs URL as a pointer, not on quoted docs text.
- GitHub HTML pages are partially blocked (Swarm page returned body length 15); READMEs were fetched through the GitHub API instead — same content, no HTML.
- A2A versioning is ambiguous in the live spec: repo release v1.0.1 (2026-05-28) but the spec's example service parameter carries `A2A-Version: 0.3` (observed). Treated as a spec-documentation inconsistency, not resolved.
- Star counts are a rough maturity proxy only and are not used in any selection argument.
- No performance benchmark was run for any framework; this document grades mechanisms, not throughput claims.

---

## 7. Bottom line

The strongest 2026 stack for H2 is not a framework — it is **four protocol shapes**:

1. **Journal + memoized phases** (Restate/Inngest/DBOS) → durable resume without duplicated side effects.
2. **Task lifecycle + subscribe + idempotency** (A2A, MCP tasks) → completion as a durable fact.
3. **`invoke_agent`/`execute_tool` spans + budgets** (OTel GenAI) → observability that survives vendor churn.
4. **Phase-gated spec packets** (Spec Kit, Agents SDK handoffs) → work that can be verified instead of narrated.

Everything else in the catalog is either an implementation of those shapes, a sandbox boundary, or an eval harness. H2's v7 should implement the four shapes on SQLite and its existing session API, and import **no framework code** — the license and abandonment data in §1 is the reason.
