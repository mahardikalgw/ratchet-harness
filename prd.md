# PRD: Ratchet — A Spec-Driven Engineering Harness for Any Model

**Document status:** Draft v0.1 (working name — rename freely)
**Author:** Otongs (Muhammad Kharisma Mahardika)
**Date:** September 15, 2026
**Target runtime:** Rust, CLI-first, single static binary

---

## 1. Executive Summary

Ratchet is a Rust CLI harness for AI-assisted software engineering, built around **spec-driven engineering (SDE)** as the primary control loop rather than free-form chat. Unlike Claude Code, Codex CLI, or Gemini CLI — which are tightly coupled to one vendor's model and largely conversation-first — Ratchet is:

1. **Spec-first, not chat-first.** Every unit of agent work is anchored to a versioned, machine-and-human-readable spec artifact (requirements → design → task plan → verification criteria) that persists in the repo, not in a disposable chat transcript.
2. **Model-agnostic by construction.** A provider abstraction layer lets any model — Claude (official Anthropic API or third-party/OpenRouter-style resellers), DeepSeek, MiMo, Qwen, GLM/Zai, local Ollama models, or any OpenAI-compatible endpoint — sit behind the same agent loop, selected per-task by capability and cost.
3. **A harness in the 2026 sense of the term**, not a prompt wrapper: a deliberately engineered system of constraints, verification loops, permissions, memory, and observability that the industry now calls **harness engineering** — the discipline of designing the environment around the model so agent mistakes get structurally ratcheted out rather than re-prompted around each time.

This document defines product scope, the standards Ratchet builds on, system architecture, functional/non-functional requirements, and a phased roadmap.

---

## 2. Why Now — Market & Standards Context (2026)

Three concurrent shifts make this the right moment for a spec-driven, model-agnostic harness:

**2.1 "Harness engineering" has become its own discipline.** Through 2026 the industry converged on the idea that agent reliability is now bottlenecked by the *harness* (tool orchestration, verification loops, context/memory management, guardrails, observability) rather than by raw model capability — summarized industry-wide as "Agent = Model + Harness." Production teams report that a well-built harness, not the underlying model, is what determines whether agent-written code is trustworthy enough to merge. The discipline's core practice is sometimes called the "engineering ratchet": whenever an agent makes a mistake, the fix is a structural constraint that makes that exact class of mistake impossible next time, rather than a prompt tweak.

**2.2 Spec-driven development has matured into a named category with competing implementations**, but no dominant, vendor-neutral, compiled-binary tool. Ecosystem entrants include markdown-based agent-behavior specs, lightweight spec formats designed to stay under a few thousand tokens for both humans and agents, and full toolkits that take a project from a one-line brief through PRD, task breakdown, and quality gates. Nearly all of today's SDD tooling is TypeScript/Python and lives as a plugin on top of someone else's proprietary agent (Claude Code, Cursor, Copilot) — there is real room for an independent, fast, single-binary harness that treats spec artifacts as the source of truth and treats the model as a swappable execution backend.

**2.3 Interoperability protocols have stabilized enough to build against.** Two protocols matter most for a CLI harness:
- **MCP (Model Context Protocol)** — now a vendor-neutral, Linux-Foundation-governed standard for exposing tools, resources, and prompts to an agent over JSON-RPC (stdio or HTTP/SSE). This is the right layer for Ratchet's own tool system and for consuming third-party MCP servers.
- **ACP (Agent Client Protocol)** — an LSP-inspired protocol, now at v1 with SDKs including Rust, that standardizes how editors/IDEs talk to coding agents, independent of which agent or model is behind it. Building Ratchet's agent core against ACP means any ACP-aware editor (Zed and others) can drive Ratchet natively, without Ratchet needing its own editor plugins.

Separately, **A2A (Agent-to-Agent)** exists for agent-to-agent delegation and is out of scope for v1, but the architecture should not preclude exposing a Ratchet task as an A2A-callable peer later.

**2.4 Multi-provider Rust LLM clients already exist as a foundation, not a gap to fill from zero.** Rust crates in this space (e.g., a native-protocol multi-provider client, and several Tower-middleware-based unified clients) already ship first-class support for Anthropic, DeepSeek, MiMo, OpenAI-compatible endpoints, OpenRouter, and local Ollama models, with retry/fallback middleware patterns. Ratchet's provider layer should build on this prior art (via a vendored/forked dependency or a thin internal trait over one of these crates) rather than reinvent per-provider HTTP clients.

---

## 3. Vision & Positioning

**Vision:** *Specs are the source of truth; models are interchangeable labor.* A team should be able to point Ratchet at a spec, let it plan and execute with whichever model is best/cheapest/available for each step, and get a reviewable, auditable trail of what changed and why — independent of which AI vendor is in favor this quarter.

**Positioning statement:**

> For engineering teams and independent developers who are tired of re-explaining intent in chat and locked into a single model vendor, Ratchet is a Rust CLI harness that drives AI coding agents from versioned specs instead of conversations. Unlike Claude Code, Codex CLI, or Cursor, Ratchet treats the model as a pluggable backend (Claude, DeepSeek, MiMo, GLM, Qwen, or any OpenAI-compatible/local endpoint) and treats the harness itself — verification loops, guardrails, memory, observability — as the product.

**Non-goals for this product (explicitly not trying to be):**
- Not a GUI IDE or an editor replacement (though it should be usable *from* an editor via ACP).
- Not a hosted multi-tenant SaaS in v1 — local-first, single-user CLI, with a path to a team/server mode later.
- Not a new foundation-model training effort — Ratchet consumes models, it doesn't build them.
- Not trying to reimplement A2A-style multi-agent swarms in v1; single-agent-per-task with optional sub-agent delegation is enough for the first releases.

---

## 4. Target Users & Personas

| Persona | Description | Core need |
|---|---|---|
| **Independent full-stack engineer** (Otongs's own primary use case) | Runs multiple concurrent client/internal projects (multi-tenant accounting apps, CRM, inventory apps, PR-review bots), often switching model providers for cost reasons | Wants one harness that works the same way across projects and doesn't lock him into one model vendor's pricing or availability |
| **Small engineering team / agency** | 3–15 engineers shipping several client codebases in parallel | Wants specs as the artifact that survives engineer turnover and AI-vendor turnover; wants an audit trail for what an agent changed and why |
| **Cost-sensitive / regional developer** | Works where Western frontier-model pricing or availability is a real constraint (e.g., Indonesian market, government/procurement clients with data-residency or budget rules) | Wants first-class support for cheaper/regional models (DeepSeek, MiMo, Qwen, GLM) without being a second-class citizen behind Claude |
| **Platform/tooling-minded engineer** | Wants to extend the harness itself — custom verification gates, custom tools, org-specific policy | Needs a clean plugin/skill and MCP-server extension surface, not a monolith |

---

## 5. Product Principles

Adapted from 2026 harness-engineering practice into concrete product commitments:

1. **The Ratchet, not the reprompt.** Every recurring agent failure mode gets converted into a structural constraint (a lint rule, a schema, a required verification step, a permission boundary) — never just a note added to a prompt.
2. **Spec is the unit of truth, chat is disposable.** Conversation logs are debugging artifacts. The spec, plan, task graph, and verification results are the artifacts that get committed and reviewed.
3. **Model-neutral core, provider-specific adapters.** No feature may assume Claude-only or OpenAI-only capabilities in the core loop; provider-specific capabilities (e.g., extended thinking, native tool-call formats) are additive, not load-bearing.
4. **Five-layer harness discipline**, applied deliberately rather than left implicit:
   - *Tool orchestration* — what the agent can do (file edits, shell, tests, MCP tools)
   - *Verification loops* — how each change is checked before it's considered "done" (compile, tests, lints, spec-conformance diff)
   - *Context & memory* — what the agent knows at each step, and what's deliberately kept out
   - *Guardrails* — permission boundaries, sandboxing, approval gates for risky actions
   - *Observability* — cost, token, and outcome tracking per task, per model, per project
5. **Human steers, agent executes, spec arbitrates.** Disagreements between what a human says in the moment and what the committed spec says are surfaced, not silently resolved by the agent.
6. **Portable by default.** Single static binary, no required daemon, works offline against local models, works in CI without a GUI.

---

## 6. Core Methodology: The Spec-Driven Loop

Ratchet formalizes a repo-resident artifact chain. Each stage produces a durable file under `.ratchet/` (or user-configured directory), versioned in git like code:

1. **Intent** (`intent.md`) — a short human-written problem statement (what and why), optionally dictated conversationally and captured by Ratchet.
2. **Spec** (`spec/<feature>.spec.md`) — structured requirements: goals, non-goals, acceptance criteria, constraints. Deliberately kept small per-feature (target: readable in one sitting) rather than one giant PRD, mirroring current lightweight-spec practice in the ecosystem.
3. **Plan** (`plan/<feature>.plan.md`) — agent-generated (human-approved) technical design: affected modules, data model changes, sequencing, risk notes.
4. **Tasks** (`tasks/<feature>.tasks.yaml`) — a task graph the agent executes against, each task carrying its own acceptance check (test to pass, lint to satisfy, behavior to demonstrate).
5. **Execution** — the agent works task-by-task inside the harness's tool sandbox, with each task closing only when its verification step passes.
6. **Verification report** (`verify/<feature>.report.md`) — machine-generated summary: what changed, what was tested, what was *not* covered, spec-conformance diff (does the diff satisfy every acceptance criterion in the spec, yes/no per line item).
7. **Review gate** — human approval step before merge; Ratchet surfaces the plan-vs-actual delta, not just the diff, so review time goes to intent-conformance rather than re-reading every line.

This loop is deliberately compatible with existing SDD conventions (`agents.md`-style agent-behavior files, OpenSpec-style pre-code alignment) so Ratchet can *consume* specs written by other SDD tools, not just its own.

---

## 7. System Architecture

### 7.1 Workspace layout (Rust, cargo workspace)

```
ratchet/
├── crates/
│   ├── ratchet-cli/          # binary entrypoint, arg parsing, TUI
│   ├── ratchet-core/         # agent loop, task graph executor, state machine
│   ├── ratchet-spec/         # spec/plan/task file format, parser, validator
│   ├── ratchet-providers/    # model provider abstraction + adapters
│   ├── ratchet-tools/        # built-in tools: fs edit, shell, test-runner, git
│   ├── ratchet-mcp/          # MCP client (consume external MCP servers)
│   ├── ratchet-acp/          # ACP server (so editors can drive Ratchet)
│   ├── ratchet-sandbox/      # permissions, approval gates, execution sandboxing
│   ├── ratchet-memory/       # context assembly, compaction, project knowledge store
│   └── ratchet-observability/# cost/token/outcome tracking, structured logs, evals
└── Cargo.toml
```

### 7.2 Agent execution model

Follows the **Agent Loop** pattern (the dominant pattern across today's open-source agents) with an explicit state machine rather than an unbounded loop, so a task is always in one of: `planning → executing → verifying → blocked(needs-approval) → done → failed`. A hybrid escape hatch to a graph/flow executor is available for tasks whose plan has real branching (e.g., "try approach A, fall back to B if tests fail").

### 7.3 Model provider abstraction layer

A single `ModelProvider` trait in `ratchet-providers`, implemented per backend:

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse>;
    async fn stream(&self, req: ChatRequest) -> Result<ChatStream>;
    fn capabilities(&self) -> ProviderCapabilities; // tool-calling, vision, thinking, max context
    fn cost_model(&self) -> CostModel;               // $/token in, out, cached
}
```

- **Official-provider adapters:** Anthropic (Claude), DeepSeek, MiMo, Alibaba/Qwen, Zhipu/GLM (Zai), Moonshot/Kimi, OpenAI, Google Gemini — each using its native protocol where that unlocks real capability (e.g., Anthropic extended thinking, provider-specific tool-call formats), falling back to an OpenAI-compatible shim where the vendor only exposes that.
- **Third-party/reseller adapters:** OpenRouter and similar aggregators, treated as just another `ModelProvider` implementation so a user can point at either the official Anthropic endpoint or a compatible reseller without changing anything upstream.
- **Local adapters:** Ollama / any OpenAI-compatible local server, for offline or air-gapped use (relevant for government-procurement engagements with data-residency constraints).
- Build on an existing native-protocol multi-provider Rust crate (one already ships adapters for Anthropic, DeepSeek, MiMo, OpenRouter, and 20+ others) as the underlying HTTP/protocol layer, wrapped by Ratchet's own trait so provider swaps never touch `ratchet-core`.

**Per-task model routing:** the task graph can pin a model per task (e.g., "planning tasks → Claude for reasoning quality; bulk boilerplate tasks → DeepSeek for cost"), or leave it to a routing policy (cheapest-that-meets-capability-requirements, fastest, or fixed default), configurable in `ratchet.toml`.

### 7.4 Tool orchestration & MCP

Built-in tools (file read/write/patch, shell exec, test runner, git operations, spec/task CRUD) are implemented natively in Rust for speed and sandboxing control. External capabilities are added by connecting to MCP servers as an MCP *client* — Ratchet does not require every capability to be reinvented; it inherits the growing MCP ecosystem.

### 7.5 ACP server mode

Ratchet also runs as an **ACP agent**, so editors that speak ACP can drive it as their backend agent, with the editor rendering Ratchet's plan-before-action and approval prompts through the editor's own UI rather than Ratchet needing a GUI of its own.

### 7.6 Sandbox & guardrails

- Filesystem writes scoped to an explicit allow-list of paths per project.
- Shell execution behind an approval gate by default for anything outside a configured safe command list (test runners, formatters, linters are pre-approved; arbitrary shell requires human confirmation).
- Network access from tools (not from the model provider calls themselves) is deny-by-default.
- Every guardrail trip is logged as a candidate "ratchet" — a structured note asking whether this should become a permanent rule.

### 7.7 Context & memory

- **Working context**: only the current task's spec, relevant file slices, and task history — not the whole repo.
- **Project memory**: a persistent, queryable store (architecture decisions, prior verification reports, known gotchas) that's summarized into context rather than dumped whole, following the "agent decides what's worth preserving" compaction approach rather than a fixed token-budget truncation.
- **Context firewalls**: raw tool output (large logs, big file dumps) is kept outside the model's context by default and fetched in fragments on demand, rather than always inlined.

### 7.8 Observability

Structured per-task records: model used, tokens in/out, cost, wall-clock time, verification pass/fail, number of human interventions. Aggregated into project-level and provider-level dashboards (`ratchet report`), enabling the cost-per-merged-change and time-to-verified-task metrics that 2026 engineering-leadership guidance treats as the baseline harness KPIs.

---

## 8. Functional Requirements

### P0 — must ship in v1
- `ratchet init` — scaffold `.ratchet/` structure in an existing repo.
- `ratchet spec new/edit/validate` — create and lint spec files against the spec schema.
- `ratchet plan` — generate a plan from a spec using the configured model; human-editable before approval.
- `ratchet run <task-id>` — execute one task through the agent loop with live verification.
- `ratchet run --all` — execute a task graph end-to-end, pausing at approval gates.
- Provider config: at minimum Anthropic (official + OpenRouter-style third-party), DeepSeek, MiMo, and one generic OpenAI-compatible adapter, selectable via `ratchet.toml` and per-task overrides.
- Built-in tools: file patch, shell (gated), test runner detection (cargo/npm/pytest/etc.), git diff/commit.
- `ratchet verify` — run the verification report independent of execution, for CI use.
- Approval-gate UX in the CLI (accept/reject/edit-and-retry per proposed action).
- Cost/usage report per run and per provider.

### P1 — near-term follow-on
- MCP client support for external tool servers.
- ACP server mode for editor integration.
- Project memory store with compaction.
- Routing policies (cost-optimized, capability-optimized, fixed).
- `ratchet import` — ingest specs from other SDD tool formats (OpenSpec, agents.md-style files).

### P2 — later
- Multi-agent sub-task delegation within a single Ratchet run (planner agent + implementer agent + reviewer agent, each possibly on a different model).
- Optional lightweight server/daemon mode for team dashboards.
- A2A exposure of a Ratchet task as a callable peer agent.
- Plugin SDK for custom verification gates and custom tools (WASM or dynamic-lib based, TBD).

---

## 9. Non-Functional Requirements

- **Performance:** cold start under 100ms for CLI commands that don't call a model; binary size reasonable for a single static Rust build; no required background daemon for core workflows.
- **Portability:** builds for Linux, macOS, Windows; static-musl build option for containerized/CI use.
- **Security:** no plaintext API keys in repo-committed files; credentials read from OS keychain or env vars; sandboxed tool execution by default.
- **Reliability:** every model call wrapped in retry/fallback middleware (respecting each provider's rate limits); a provider outage on one backend should not block work that's routed to another.
- **Auditability:** every agent action traceable to the spec/task that authorized it.
- **Offline capability:** must be usable against local models (Ollama or compatible) with zero network dependency for the core loop.
- **Extensibility:** adding a new model provider should require implementing one trait, not touching core logic.

---

## 10. CLI UX Sketch

```
ratchet init                          # scaffold .ratchet/ in current repo
ratchet spec new billing-reminders    # create spec/billing-reminders.spec.md
ratchet plan billing-reminders        # generate + review plan
ratchet tasks billing-reminders       # show/edit generated task graph
ratchet run billing-reminders         # execute full task graph, gated
ratchet run billing-reminders --task T-03 --model deepseek:deepseek-chat
ratchet verify billing-reminders      # standalone verification (CI-friendly)
ratchet report --since 7d             # cost/outcome dashboard across providers
ratchet provider add mimo --key-env MIMO_API_KEY
ratchet provider add claude --via openrouter --key-env OPENROUTER_KEY
```

---

## 11. Configuration Model (`ratchet.toml`, sketch)

```toml
[project]
name = "aplikasi-akuntansi-multitenant"

[providers.claude]
kind = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"

[providers.claude_via_openrouter]
kind = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
model = "anthropic/claude-opus-5"
api_key_env = "OPENROUTER_API_KEY"

[providers.deepseek]
kind = "deepseek"
api_key_env = "DEEPSEEK_API_KEY"

[providers.mimo]
kind = "mimo"
api_key_env = "MIMO_API_KEY"

[routing]
default = "deepseek"
planning_tasks = "claude"
policy = "capability_then_cost"

[sandbox]
allowed_paths = ["src/", "tests/", "docs/"]
shell_allowlist = ["cargo test", "cargo fmt", "cargo clippy"]
network = "deny"
```

---

## 12. Roadmap

| Phase | Timeframe (indicative) | Scope |
|---|---|---|
| **Phase 0 — Spike** | 2–3 weeks | Provider trait + 3 adapters (Claude, DeepSeek, MiMo) working end-to-end on a trivial "edit file, run test" loop; no spec engine yet |
| **Phase 1 — MVP (P0 scope)** | 6–8 weeks | Full spec→plan→tasks→execute→verify loop, CLI approval gates, cost reporting |
| **Phase 2 — Ecosystem hooks (P1)** | 6–8 weeks | MCP client, ACP server mode, project memory, routing policies, spec import |
| **Phase 3 — Team scale (P2)** | Ongoing | Multi-agent delegation, plugin SDK, optional server mode |

Suggested first dogfood target: point Ratchet at one of Otongs's existing in-flight repos (e.g., the multi-tenant accounting app or Handchannel) for a real, low-risk feature slice.

---

## 13. Success Metrics

- **Time-to-verified-task**: median wall-clock from `ratchet run` start to a task passing verification.
- **Cost per merged change**, broken out by provider — the core justification for multi-model routing.
- **Human-intervention rate**: approval-gate stops per task, trending down over time as guardrails mature (the "ratchet" signal working).
- **Spec-conformance rate**: % of acceptance criteria satisfied without manual patch-up.
- **Provider portability check**: same spec, run against at least two different providers, producing passing implementations — proof the abstraction layer isn't leaking model-specific assumptions.

---

## 14. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Provider APIs (esp. MiMo, DeepSeek) change faster than an internal adapter can track | Build on an actively maintained multi-provider Rust crate as the transport layer instead of hand-rolling every HTTP client; keep Ratchet's own trait thin |
| Spec format becomes a second thing to maintain alongside code, gets stale | Keep specs small per-feature (not one giant PRD) and make `ratchet verify` fail loudly when code drifts from spec |
| Sandboxing/guardrails add friction that gets bypassed in practice | Make the safe path the fast path — pre-approve common safe commands (test/format/lint) so approval gates only fire for genuinely risky actions |
| Scope creep toward "yet another agent framework" | Explicitly park multi-agent orchestration and server mode to P2; v1 stays single-agent, single-binary, CLI-only |
| ACP/MCP ecosystem still moving (both hit "v1 stable" only recently in 2026) | Isolate protocol code in dedicated crates (`ratchet-acp`, `ratchet-mcp`) so a spec revision doesn't touch core |

---

## 15. Open Questions

1. Final product/binary name (Ratchet is a working name chosen for its harness-engineering resonance — check crates.io/npm/GitHub availability before committing).
2. Should spec files be Markdown-with-frontmatter (human-friendliest, matches ecosystem convention) or a stricter schema (YAML/TOML, easier to validate mechanically)? Leaning Markdown-with-frontmatter for v1, mechanical validation via a parser rather than forcing a rigid format.
3. Team/server mode: out of scope for v1, but should the on-disk spec/task format be designed now so a future server mode doesn't require a breaking migration?
4. How much of the sandbox should be OS-level (namespaces/containers) vs. process-level convention-based restriction, given the "just CLI, no required daemon" constraint?

---

## 16. References (background research, Sept 2026)

- ai-boost/awesome-harness-engineering — GitHub curated list on harness engineering patterns, tools, and 2026 milestones
- mahonzhan/awesome-agent-harness — GitHub, timeline of harness-engineering coinage and "Agent = Model + Harness" formulation
- Augment Code — "Harness Engineering for AI Coding Agents" guide
- Faros AI — "Harness Engineering: A Guide to AI Coding Agents" (five-layer harness model)
- engineering4ai/awesome-spec-driven-development — GitHub curated list of SDD tools and standards (OpenSpec, Spec Kit, agents.md, lean-spec, etc.)
- getstream.io / blog.agentailor.com / zuplo.com / digitalapplied.com — 2026 surveys of MCP, ACP, and A2A protocol status
- ai-sdk.dev community ACP provider docs
- jeremychone/rust-genai (and forks) — native-protocol multi-provider Rust LLM client with existing MiMo/DeepSeek/Anthropic/OpenRouter adapters
- crates.io: agentix, llmkit-rs, edgequake-llm, llm-connector, rsllm, cnctd_ai — additional Rust multi-provider LLM client prior art

---

*End of draft. Next step: review Section 8 (P0 scope) and Section 15 (open questions) together, then move to Phase 0 spike.*
