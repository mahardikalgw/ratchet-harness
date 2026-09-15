# How Ratchet works

Architecture, the spec-driven loop, and notes for working on the codebase.

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

---

## Development

```bash
cargo check                       # type-check
cargo test                        # run the test suite
cargo clippy --all-targets        # lint
cargo fmt --all                   # format
cargo build --release             # optimized binary
```

### Releasing

```bash
scripts/release.sh 0.4.0            # bump, test, commit, tag, push
scripts/release.sh 0.4.0 --dry-run  # show what would happen
```

The version lives in exactly one place — `[workspace.package] version` in the
root `Cargo.toml` — and all 13 crates inherit it via `version.workspace = true`.
The binary reads it at compile time (`env!("CARGO_PKG_VERSION")`), so
`ratchet --version` cannot drift from the release once the tag matches.

Two independent guards keep it that way:

| Guard | Where | Catches |
|---|---|---|
| version consistency job | every CI run | a crate that stopped inheriting the workspace version |
| `verify` job | every release | a tag that disagrees with `Cargo.toml` |

The release job runs first and fails the whole pipeline before anything is
built, so a mislabelled release is impossible rather than merely unlikely. The
script exists because the manual version of this — edit, commit, tag, push —
is exactly how the two drifted apart in the first place.

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

---

## Portability

Linux, macOS, and Windows via a single binary with no required daemon. Static
musl builds for containers:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

---

---

See also: [Configuration](configuration.md) · [Extending](extending.md) · [back to README](../README.md)
