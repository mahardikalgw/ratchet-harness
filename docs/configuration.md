# Configuration and providers

Reference for `ratchet.toml`, model providers, credentials, and the
sandbox that bounds what the agent may touch.

## `ratchet.toml` reference

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

---

## Skills and agent instruction directories

Directories that hold agent instructions — `.agents`, `.claude`, `.cursor`,
`.codex`, `.gemini`, `.continue`, `.aider`, `.pi` — are detected, reported, and
made **readable but not writable**:

```
$ ratchet init my-project

🔍 Memeriksa proyek…
   bahasa        : Rust
   folder sumber : src
   skills        : .agents (bisa dibaca, tidak bisa diubah)
   tidak diizinkan: .github (tambahkan manual kalau perlu)

   2 skill ditemukan:
     • rust-best-practices
     • rust-testing
```

```toml
[sandbox]
allowed_paths = ["src", ".ratchet"]   # read + write
read_only_paths = [".agents"]         # read only
```

The read/write split matters. Skills are useful project context, but an agent
that can *write* them can rewrite its own instructions — so `file_write` and
`file_patch` against a read-only path are refused with an error that says why.

Skill files (`.agents/skills/*/SKILL.md`) are listed by `init` and appear in
the repository map the agent is shown, so it discovers them without being told
where they live.

Directories that exist but are neither source nor agent tooling — `.github`,
`.vscode`, `.idea`, `.devcontainer` — are reported but not added to the write
scope. Add them by hand if a task genuinely needs to edit, say, CI workflows.

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

---

## Spec import

Ingest specs written by other SDD tools:

```bash
ratchet import ./agents.md --format agents.md
ratchet import ./openspec.yaml --format openspec
ratchet import ./feature.md              # auto-detect
```

Imported specs are normalised to Ratchet's own format and written to
`.ratchet/spec/`.

---

---

See also: [Extending](extending.md) · [How it works](internals.md) · [back to README](../README.md)
