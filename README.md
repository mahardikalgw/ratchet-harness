# Ratchet

**A spec-driven engineering harness for any model.**

[![CI](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/ci.yml/badge.svg)](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/ci.yml)
[![Release](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/release.yml/badge.svg)](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Ratchet is a Rust CLI harness for AI-assisted software engineering, built around
**spec-driven engineering (SDE)** as the primary control loop rather than
free-form chat. It treats the model as a pluggable backend and treats the
harness itself — verification loops, guardrails, memory, observability — as the
product.

> Specs are the source of truth; models are interchangeable labor.

---

## Status

| Phase | Scope | Status |
|---|---|---|
| **Phase 0 — Spike** | Provider trait + adapters, edit/test loop | ✅ Done |
| **Phase 1 — MVP (P0)** | spec→plan→tasks→execute→verify, agentic loop, verification gates, cost reporting | ✅ Done |
| **Phase 2 — Ecosystem hooks (P1)** | MCP client, ACP server, project memory, routing policies, spec import, streaming | ✅ Done |
| **Phase 3 — Team scale (P2)** | Multi-agent delegation, plugin SDK, dashboard/server mode, A2A | ✅ Done |

See **Model requirements** below before expecting an agentic run to succeed —
the harness is complete, but it needs a model that can actually call tools.

### Known limitations

- **Model capability is the binding constraint.** Ratchet requires a model with
  native tool-calling support. See *Model requirements* below for what was
  measured.
- ACP `agent/run` spawns the run in the background and reports status via
  `agent/status`; streaming progress is not yet emitted.
- Approval prompts fall back to a safe denial when no TTY is attached, so
  unattended runs cannot silently execute non-allow-listed shell commands.
- Provider adapters are written against the documented APIs and have been
  exercised end-to-end against Ollama locally; the hosted adapters have not
  been run against live endpoints in CI.

---

## Model requirements

Ratchet drives work through **tool calls**. A model that cannot emit them will
only narrate the work, and the harness will (correctly) report that nothing
changed.

Measured behaviours during development:

| Model | Tool calling | Agentic coding |
|---|---|---|
| `qwen2.5:1.5b` (local) | ✅ native | ⚠️ too weak — loses the thread after one step |
| `mistral:7b` (local) | ❌ none — emits pseudocode prose | ❌ unusable for edits |
| `qwen2.5:7b`+ / `llama3.1:8b`+ (local) | ✅ | ✅ expected; not verified here (disk limit) |
| Claude / DeepSeek / GPT-class (hosted) | ✅ | ✅ expected |

If a run reports *“the model made no tool calls”*, the model is the problem, not
the spec. Ratchet also recovers tool calls that a model emits as fenced text,
which helps mid-tier models but cannot compensate for a model that has no tool
training at all.

### What the harness does to help weak models

- Injects a deterministic **repository map** so the model never has to guess
  file paths, and cannot invent placeholders like `/path/to/file.txt`.
- **Normalises** absolute-looking paths (`/src/lib.rs`) to project-relative
  ones instead of silently escaping the sandbox.
- Accepts **argument aliases** (`file`, `file_path`, `contents`, ...) rather
  than failing on naming differences.
- Returns **actionable tool errors** (naming the allowed scope) so a model can
  correct itself on the next turn.
- Recovers **tool calls emitted as text** in fenced code blocks.

---

## Workspace Layout

```
ratchet/
├── crates/
│   ├── ratchet-cli/          # binary entrypoint, arg parsing, commands
│   ├── ratchet-core/         # agent loop, task graph executor, delegation, review
│   ├── ratchet-spec/         # spec format, parser, validator, task graph
│   ├── ratchet-providers/    # ModelProvider trait + adapters + resilience
│   ├── ratchet-tools/        # built-in tools: fs, shell, search, test-runner, git
│   ├── ratchet-mcp/          # MCP client (consume external MCP servers)
│   ├── ratchet-acp/          # ACP server (editors drive Ratchet)
│   ├── ratchet-plugins/      # plugin SDK: custom gates and tools
│   ├── ratchet-a2a/          # A2A protocol types
│   ├── ratchet-server/       # HTTP dashboard + A2A endpoint
│   ├── ratchet-sandbox/      # permissions, approval gates
│   ├── ratchet-memory/       # context assembly, project knowledge store
│   └── ratchet-observability/# cost/token/outcome tracking, reporting
└── Cargo.toml
```

---

## Installation

### Install script (macOS, Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/mahardikalgw/ratchet-harness/main/install.sh | sh
```

Installs a prebuilt binary to `~/.local/bin` (or `/usr/local/bin` when it is
already writable), verifies the SHA256 checksum when the release publishes one,
and prints a `PATH` hint if the directory is not on your `PATH`.

Options are passed through the environment:

```bash
# a specific version
curl -fsSL https://raw.githubusercontent.com/mahardikalgw/ratchet-harness/main/install.sh \
  | RATCHET_VERSION=v0.1.0 sh

# a specific directory
curl -fsSL https://raw.githubusercontent.com/mahardikalgw/ratchet-harness/main/install.sh \
  | RATCHET_INSTALL_DIR="$HOME/bin" sh
```

Prebuilt for `x86_64`/`aarch64` on Linux (static musl) and macOS, plus
`x86_64` Windows.

### Cargo

```bash
cargo install --git https://github.com/mahardikalgw/ratchet-harness ratchet-cli
```

### From source

```bash
git clone https://github.com/mahardikalgw/ratchet-harness
cd ratchet-harness
cargo build --release
# binary at target/release/ratchet
```

Static musl build for containers and CI:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### Verify

```bash
ratchet --version
ratchet --help
```

### Uninstall

```bash
rm -f ~/.local/bin/ratchet
rm -rf ~/.ratchet           # cached credentials (OS keychain entries persist)
```

---

## Quick Start

Just start talking. There is no markdown to edit:

```
$ ratchet

Ratchet — ngobrol dulu, baru dikerjakan
› buatkan aku toko online sederhana

🤖 Baik — "buatkan aku toko online sederhana".
   Saya tanya beberapa hal dulu supaya spec-nya tepat.

🤖 Dua hal yang mengubah desainnya:
[1/2] Jual produk fisik, digital, atau keduanya?
   1. fisik
   2. digital
   3. keduanya
Pilih (enter = fisik): 1
[2/2] Payment gateway apa yang dipakai?
Jawab: midtrans

🤖 Saya sudah cukup. Ini spec yang saya usulkan:

   Toko Online Sederhana  (id: toko-online)

   Tujuan
     • Menampilkan katalog produk
     • Checkout satu produk

   Kriteria diterima
     AC-1 Fungsi katalog tersedia      (cek: src/lib.rs berubah)
     AC-2 Test lulus                   (cek: cargo test)

   ✓ 2 kriteria bisa dicek otomatis

   enter = setuju · ketik perubahan untuk revisi · /batal
›

   ✓ spec disimpan: ./.ratchet/spec/toko-online.spec.md
🤖 Sekarang saya susun rencana kerjanya…

   Rencana
     T-1 Implement the catalogue

   enter = jalankan · /batal
›

🤖 Mulai kerja. Saya laporkan kalau sudah selesai.

🤖 Selesai. Saya menambahkan katalog produk di src/lib.rs beserta test-nya,
   dan seluruh kriteria otomatis sudah lolos.

   ✅ AC-1 — Fungsi katalog tersedia
   ✅ AC-2 — Test lulus
```

That is the whole workflow. The model asks what it needs to know, proposes a
spec with **checkable** acceptance criteria, and only writes code once you
approve. The spec and task graph still land on disk as durable artifacts — you
just never have to author them by hand.

### What happens behind the conversation

```
your request
    ↓
discovery      model asks questions until it can write verifiable criteria
    ↓
spec           proposed for your approval, then written to .ratchet/spec/
    ↓
plan           task graph proposed for your approval
    ↓
execution      tools run, each task verified as it closes
    ↓
verification   every criterion checked against real evidence
    ↓
report         plain-language summary of what changed and what did not
```

### Session commands

| Command | Does |
|---|---|
| *(type anything)* | starts a new request, or revises the current spec |
| `enter` | approve the current spec or plan |
| `/spec` | show the proposed spec again |
| `/status` | current phase |
| `/reset` | discard and start over |
| `/help` | help |
| `/keluar` | exit |

### Non-interactive use

Every step is also a command, for CI and scripting:

```bash
ratchet spec new billing-reminders    # or author the spec yourself
ratchet plan billing-reminders
ratchet run billing-reminders --all
ratchet verify billing-reminders
```

### Adding Ratchet to an existing repository

`ratchet init` is designed for this. It looks at the repository first and
writes a config that fits, so the sandbox permits the commands your project
actually uses:

```
$ cd existing-project
$ ratchet init existing-project

🔍 Memeriksa proyek…
   bahasa        : Rust + Node/TypeScript
   folder sumber : src, components
   diabaikan     : node_modules, target (vendor/generated)

✅ Siap. Dibuat:
   ./ratchet.toml
   ./.ratchet/
```

Recognised ecosystems: Rust, Node/TypeScript, Python, Go, Ruby, Java, PHP,
Make — and combinations of them. For each it derives the safe command
allow-list (`cargo test`, `pytest`, `go test ./...`, …) and only includes
directories that exist.

What it will **not** do:

- touch, move, or reformat any existing file
- add vendored or generated directories (`node_modules`, `target`, `dist`,
  `build`, `.venv`, `vendor`, …) to the write allow-list
- overwrite an existing `ratchet.toml` — it refuses and tells you

Everything it creates is additive and easy to remove:

```
ratchet.toml     config
.ratchet/        specs, plans, task graphs, verification reports
```

If the detected allow-list is wrong for your project, edit `ratchet.toml` —
it is a plain, commented file. Run `ratchet doctor` afterwards to confirm.

### First-time setup

```bash
ratchet init my-project
cd my-project

ratchet provider add claude --kind anthropic --key-env ANTHROPIC_API_KEY
ratchet provider test claude          # confirm the credential works before anything else

ratchet                               # start the conversation
```

`init` deliberately configures no providers, so routing is set the moment you
add one — there is no config to hand-edit.

Provider flags cover the common cases without touching `ratchet.toml`:

```bash
ratchet provider add local --kind ollama --model qwen2.5:7b
ratchet provider add mimo  --kind mimo --model <model> --base-url https://... \
  --key-env MIMO_API_KEY
ratchet provider add claude-via-openrouter --kind anthropic --via openrouter \
  --key-env OPENROUTER_API_KEY
```

### Stuck?

```bash
ratchet doctor            # config, credentials, routing, git — with fixes
ratchet doctor --online   # also makes a live request to each provider
```

## Commands

| Command | Description |
|---|---|
| `ratchet` / `ratchet chat [prompt]` | Interactive session — describe what you want, answer questions, approve, watch it build |
| `ratchet init [name]` | Scaffold `.ratchet/` in the current repo |
| `ratchet spec new <id>` | Create a new spec |
| `ratchet spec edit <id>` | Open a spec in `$EDITOR` |
| `ratchet spec validate <path>` | Validate a spec file |
| `ratchet spec list` | List all specs |
| `ratchet plan <spec>` | Generate a technical plan from a spec |
| `ratchet tasks <spec>` | Show the task graph |
| `ratchet run <spec> [--task <id>] [--all]` | Execute tasks |
| `ratchet verify <spec>` | Run the verification report |
| `ratchet import <path> [--format <fmt>]` | Import specs from external SDD formats |
| `ratchet serve [--port <n>]` | Start the ACP server for editor integration |
| `ratchet dashboard [--port <n>] [--a2a]` | Local dashboard + A2A endpoint |
| `ratchet review <spec>` | Show the plan-vs-actual delta from the last run |
| `ratchet mcp <command> [args...]` | Connect to an MCP server |
| `ratchet report [--since <days>]` | Cost/outcome dashboard |
| `ratchet doctor [--online]` | Diagnose setup problems and print how to fix them |
| `ratchet provider add/login/logout/test/list/remove` | Manage providers and credentials |

---

## Spec-Driven Loop

Each stage produces a durable artifact under `.ratchet/`, versioned in git:

```
intent.md
  → spec/<feature>.spec.md      # goals, non-goals, acceptance criteria, constraints
  → plan/<feature>.plan.md      # agent-generated technical design
  → tasks/<feature>.tasks.yaml  # task graph with per-task acceptance checks
  → (execution)
  → verify/<feature>.report.md  # machine-generated conformance report
```

### Spec format

Markdown with YAML frontmatter. Acceptance-criteria lines may carry an
optional machine-readable annotation naming how they are verified:

```markdown
---
id: billing-reminders
title: "Billing Reminders"
status: draft
priority: high
tags: [billing]
dependencies: []
---

# Goals
- Send payment reminders before due date

# Non-Goals
- Handling actual payments

# Acceptance Criteria
- [ ] AC-1: Reminder sent 3 days before due date [verify: cargo test reminders]
- [ ] AC-2: Billing module updated              [verify-diff: src/billing/]
- [ ] AC-3: Clippy is clean                     [verify-lint: cargo clippy]
- [ ] AC-4: Copy reads well

# Constraints
- Must respect user timezone
```

| Annotation | Meaning | Status when checked |
|---|---|---|
| `[verify: <command>]` | Run the command; pass if output contains `test result: ok` | ✅ / ❌ |
| `[verify-lint: <command>]` | Run the command; pass if exit code is 0 | ✅ / ❌ |
| `[verify-diff: <pattern>]` | Pass if a changed file path contains the pattern | ✅ / ❌ |
| *(none)* | Not machine-checkable | 🟡 manual |

Manual criteria are **never** silently counted as passing: they are reported
separately, and `ratchet verify` exits non-zero only when an automatic check
actually fails.

---

## Providers

Ratchet is model-agnostic. Any of these can sit behind the same agent loop:

| Kind | Adapter | Notes |
|---|---|---|
| `anthropic` | Native Anthropic API | Extended thinking, caching, streaming |
| `deepseek` | OpenAI-compatible | Low cost, streaming |
| `mimo` | OpenAI-compatible | Regional, streaming |
| `openai_compatible` | Generic | OpenRouter, resellers, any compatible endpoint, streaming |
| `ollama` | Native Ollama | Local / offline / air-gapped |

All adapters implement both `complete()` and `stream()` (server-sent events).
The agent loop currently uses `complete()`; streaming is available for callers
that want incremental output.

### Routing policies

Set in `ratchet.toml` under `[routing]`:

- `fixed` — always use the default provider
- `capability-then-cost` — pick the best capability match, then cheapest
- `cost-optimized` — cheapest provider that meets capability requirements
- `fastest` — prefer local models when available

A task can pin a model explicitly via `assigned_model`; the router honors it.

---

## Configuration (`ratchet.toml`)

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

[routing]
default = "deepseek"
planning_tasks = "claude"
policy = "capability-then-cost"

[sandbox]
allowed_paths = ["src/", "tests/", "docs/"]
shell_allowlist = ["cargo test", "cargo fmt", "cargo clippy"]
network_allowed = false
approval_policy = "interactive"

# Optional: connect external MCP servers as tools
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

API keys are read from the environment (or OS keychain); never stored in the repo.

---

## How a task runs

1. Pick a model via the routing policy (or a task's `assigned_model`).
2. Send the task plus a system prompt describing the available tools.
3. If the model requests tools, execute them (sandboxed, MCP-aware) and feed
   the results back — repeating until it stops calling tools or hits the
   12-turn cap.
4. Persist per-task token/cost metrics to `.ratchet/metrics.jsonl`.
5. Run verification and write `.ratchet/verify/<spec>.report.md`.

MCP tools are exposed to the model under `server.tool` names, so a server
named `filesystem` contributes `filesystem.read_file` and so on.

---

## Guardrails

- Filesystem writes scoped to an explicit allow-list per project
- Shell execution behind an approval gate; safe commands (test/format/lint) pre-approved
- Network deny-by-default for tools
- Every guardrail trip is logged as a candidate "ratchet"

### Resilience

Every model call is wrapped in retry-with-backoff and cross-provider failover:

- Transient failures (429, 5xx, timeouts, connection errors) are retried with
  exponential backoff and jitter.
- Permanent failures (auth, bad config, unsupported capability) are **not**
  retried — that would only burn time and tokens.
- If one provider is down, the router's ranked candidates are tried in order,
  so an outage on one backend does not block work another could serve.

### Credentials

API keys are resolved from the environment first, then the OS keychain — never
stored in the repository:

```bash
ratchet provider add claude --kind anthropic --key-env ANTHROPIC_API_KEY
ratchet provider login claude     # prompts and stores in the OS keychain
ratchet provider list             # shows which source each credential came from
```

---

## Editor Integration (ACP)

Start the ACP server and point any ACP-aware editor (Zed and others) at it:

```bash
ratchet serve --port 8765
```

Supported methods: `initialize`, `agent/run`, `agent/plan`, `agent/approve`, `agent/status`.

---

## MCP Integration

Inspect the tools and resources exposed by an MCP server:

```bash
ratchet mcp npx -y @modelcontextprotocol/server-filesystem /path/to/dir
```

The MCP client speaks JSON-RPC over stdio and supports `tools/list`, `tools/call`,
`resources/list`, and `resources/read`.

---

## Spec Import

Ingest specs written by other SDD tools:

```bash
ratchet import ./agents.md --format agents.md
ratchet import ./openspec.yaml --format openspec
ratchet import ./feature.md              # auto-detect
```

Imported specs are normalised to Ratchet's own format and written to
`.ratchet/spec/`.

---

## Reports and metrics

Every task appends one JSON line to `.ratchet/metrics.jsonl`:

```bash
ratchet report --since 7d                # text summary
ratchet report --format json             # machine-readable
ratchet report --format markdown         # for PRs / dashboards
```

---

---

## Multi-agent delegation (P2)

Each task runs as a pipeline of agents, and each role can be pinned to a
different model — reasoning-heavy steps to a strong model, bulk edits to a
cheap one.

```toml
[delegation]
review = true              # run a reviewer agent over each task's changes
max_review_rounds = 2      # how many times a rejected task is sent back

[delegation.roles]
implementer = "deepseek"
reviewer = "claude"
```

Flow per task:

```
implementer ──▶ reviewer ──reject──▶ implementer ──▶ reviewer ──approve──▶ done
                    │
                    └──accept──▶ done
```

- The reviewer sees the **diff** (tracked and untracked files) and must answer
  with `{"approved": bool, "issues": [...], "summary": "..."}`.
- Rejections feed the concrete issues back to the implementer for a bounded
  number of rounds.
- A reviewer that returns unparseable output **never** triggers a retry loop —
  it is surfaced instead, because looping on garbage burns tokens without
  converging.
- Roles have distinct capability requirements, so routing can send the planner
  to an extended-thinking model and the implementer to a tool-capable one.

---

## Plugins (P2)

Plugins are external commands speaking JSON over stdio — any language, no
in-process code, isolated crashes.

```toml
[[plugins]]
name = "coverage"
kind = "gate"              # gate | tool
command = "python3"
args = ["./plugins/coverage.py"]
criteria = ["AC-3"]        # optional: only these acceptance criteria
timeout_secs = 60
```

**Gate plugins** receive the criteria plus evidence (changed files, test
result) and return a verdict per criterion. This is the piece MCP does not
cover: MCP supplies tools, not verification semantics. A plugin verdict
overrides the built-in heuristic.

```python
# receive one JSON object on stdin, print one on stdout
{"results": [{"criterion_id": "AC-3", "status": "passed", "note": "coverage 91%"}]}
```

**Tool plugins** describe their tools and are invoked per call; they appear to
the agent as `plugin.tool`.

A gate that crashes, times out, or returns malformed output degrades to
`manual` — it can never masquerade as verified.

---

## Dashboard and A2A (P2)

```bash
ratchet dashboard --port 8788            # metrics only (read-only)
ratchet dashboard --port 8788 --a2a      # also accept delegated tasks
```

| Endpoint | Purpose |
|---|---|
| `GET /` | Self-contained HTML dashboard (no CDN, no build step) |
| `GET /api/summary` | Aggregated cost/token/outcome metrics |
| `GET /api/tasks` | Recent per-task records |
| `GET /api/specs` | Specs and their status |
| `GET /.well-known/agent.json` | A2A agent card |
| `POST /a2a` | A2A JSON-RPC (`tasks/send`, `tasks/get`, `tasks/cancel`) |

Delegating work from a peer agent:

```bash
curl -s localhost:8788/a2a -d '{
  "jsonrpc":"2.0","id":1,"method":"tasks/send",
  "params":{
    "message":{"role":"user","parts":[{"type":"text","text":"run spec:slugify"}]},
    "metadata":{"spec_id":"slugify","skill":"run-spec"}
  }}'
```

`tasks/send` returns as soon as work is accepted; poll `tasks/get` for the
outcome. Completed tasks carry `summary` and `review` artifacts. Without
`--a2a`, the endpoint advertises a closed agent and refuses all work.

## Testing without a model

Model quality is the usual reason a run disappoints, which makes it hard to
tell whether the harness is at fault. `examples/mock-model-server.py` is a
scripted, OpenAI-compatible model that always behaves the same way, so a
failure means the harness is wrong:

```bash
# terminal 1
python3 examples/mock-model-server.py --port 8899

# terminal 2
ratchet provider add mock --kind mimo --model mock-model \
  --base-url http://127.0.0.1:8899/v1 --key-env MOCK_KEY
MOCK_KEY=x ratchet run <spec> --all
```

Useful switches for exercising specific paths:

| Flag | Exercises |
|---|---|
| `--reject-first-review` | the reviewer → revision loop |
| `--fail-first 2` | retry with backoff and cross-provider failover |

---

## Development

```bash
cargo check                       # type-check
cargo test                        # run the test suite
cargo clippy --all-targets        # lint
cargo fmt --all                   # format
cargo build --release             # optimized binary
```

### CI/CD

| Workflow | Trigger | Does |
|---|---|---|
| [`ci.yml`](.github/workflows/ci.yml) | every push / PR to `main` | `fmt --check`, `clippy -D warnings`, `test`, release build and binary smoke test on Linux, macOS, and Windows; a pinned MSRV check; a static musl build |
| [`release.yml`](.github/workflows/release.yml) | a `v*` tag, or manual dispatch | cross-builds release binaries, generates `SHA256SUMS`, and publishes a GitHub Release |

Cutting a release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

That publishes binaries for `x86_64`/`aarch64` Linux (static musl),
`x86_64`/`aarch64` macOS, and `x86_64` Windows — plus the checksums the
install script verifies against.

---

## Portability

Linux, macOS, and Windows via a single binary with no required daemon. Static
musl builds for containers:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

---

## License

MIT — see [LICENSE](LICENSE).
