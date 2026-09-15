# Ratchet

**Describe what you want. Ratchet asks a few questions, writes the spec, and builds it.**

[![CI](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/ci.yml/badge.svg)](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/ci.yml)
[![Release](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/release.yml/badge.svg)](https://github.com/mahardikalgw/ratchet-harness/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

---

## What is this?

Ratchet is a command-line coding agent. You give it a request in plain language;
it asks what it needs to know, writes a specification you approve, then does the
work with tools — reading and editing files, running commands, running tests.

The difference from a chat-based agent is **what counts as done**. Every
acceptance criterion in the spec is tied to something checkable — a command that
must exit 0, or a file that must change. Ratchet verifies them and tells you
which passed, which failed, and which still need a human. It will not call
something finished because the model said so.

The second difference is that it is **not tied to one vendor**. Claude, DeepSeek,
MiMo, Qwen, GLM, a local Ollama model, or anything OpenAI-compatible all sit
behind the same loop, and different steps can be routed to different models —
reasoning to a strong one, bulk edits to a cheap one.

```
$ ratchet
› buatkan aku toko online sederhana

🤖 Dua hal yang mengubah desainnya:
[1/2] Jual produk fisik, digital, atau keduanya?  1
[2/2] Payment gateway apa yang dipakai?           midtrans

🤖 Ini spec yang saya usulkan:
   Toko Online Sederhana  (id: toko-online)
     AC-1 Fungsi katalog tersedia   (cek: src/lib.rs berubah)
     AC-2 Test lulus                (cek: cargo test)
   enter = setuju
›

   ✓ spec disimpan
🤖 Rencana: T-1 Implement the catalogue — enter = jalankan
›

🤖 Selesai. Katalog ditambahkan di src/lib.rs, seluruh kriteria lolos.
   ✅ AC-1   ✅ AC-2
```

No markdown to write by hand. The spec and task graph still land in `.ratchet/`
as durable, reviewable artifacts — you just never author them manually.

---

## Install

### macOS and Linux

```bash
curl -fsSL https://raw.githubusercontent.com/mahardikalgw/ratchet-harness/main/install.sh | sh
```

Installs to `~/.local/bin` (or `/usr/local/bin` if already writable) and
verifies the SHA256 checksum.

Prebuilt for `x86_64`/`aarch64` Linux (static musl) and macOS, plus `x86_64`
Windows.

### With Cargo

```bash
cargo install --git https://github.com/mahardikalgw/ratchet-harness ratchet-cli
```

### From source

```bash
git clone https://github.com/mahardikalgw/ratchet-harness
cd ratchet-harness
cargo build --release        # binary at target/release/ratchet
```

### Verify

```bash
ratchet --version
```

---

## Getting started

```bash
cd your-project
ratchet init your-project            # writes ratchet.toml and .ratchet/

ratchet provider add claude --kind anthropic --key-env ANTHROPIC_API_KEY
ratchet provider test claude         # confirm the credential works first

ratchet                              # start the conversation
```

`init` is built for existing repositories. It inspects the project and writes a
config that fits it — detecting the language, the source directories, and the
test command — and does not touch anything else:

```
$ ratchet init myapp

🔍 Memeriksa proyek…
   bahasa        : Rust
   folder sumber : src
   diabaikan     : node_modules, target (vendor/generated)

✅ Siap. Dibuat:
   ./ratchet.toml
   ./.ratchet/
```

It will **not**:

- modify, move or reformat any existing file
- create or edit your root `.gitignore` (machine-local state is ignored via a
  scoped `.ratchet/.gitignore` instead)
- add vendored or generated directories to the write allow-list
- overwrite an existing `ratchet.toml`

`.ratchet/` holds the specs, plans and verification reports, so it is worth
committing. Clone a repository that already has `ratchet.toml` and you can skip
`init` entirely.

### Or set it up from a local model

```bash
ollama pull qwen2.5:7b               # needs tool-calling support
ratchet provider add local --kind ollama --model qwen2.5:7b
ratchet provider test local
ratchet
```

### Something wrong?

```bash
ratchet doctor            # config, credentials, routing, git — with fixes
ratchet doctor --online   # also makes a live request to each provider
```

---

## Commands

| Command | Does |
|---|---|
| `ratchet` | Interactive session — describe, answer, approve, watch it build |
| `ratchet chat [prompt]` | Same, skipping the opening prompt |
| `ratchet init [name]` | Add Ratchet to a repository |
| `ratchet doctor [--online]` | Diagnose setup problems |
| `ratchet spec new/list/validate` | Work with spec files directly |
| `ratchet plan <spec>` | Generate a task graph |
| `ratchet tasks <spec> [--edit]` | Show or edit the task graph |
| `ratchet run <spec> [--all]` | Execute, unattended — this is what CI uses |
| `ratchet verify <spec>` | Check the working tree against the spec; exits 1 on failure |
| `ratchet review <spec>` | Plan-vs-actual delta from the last run |
| `ratchet report [--since 7d]` | Token and cost breakdown |
| `ratchet provider add/login/test/list/remove` | Manage models and credentials |
| `ratchet import <path>` | Import a spec written by another tool |
| `ratchet dashboard [--a2a]` | Local dashboard, optionally accepting A2A tasks |
| `ratchet serve` | ACP server, so editors can drive Ratchet |
| `ratchet mcp <cmd>` | Connect to an MCP server |

---

## Which models work

Ratchet drives work through **tool calls**. A model that cannot emit them will
only narrate the work, and Ratchet will correctly report that nothing changed.

| Model | Tool calling | Verdict |
|---|---|---|
| Claude, GPT, DeepSeek, Qwen, GLM (hosted) | ✅ | use these |
| `qwen2.5:7b`, `llama3.1:8b` and up (local) | ✅ | works |
| `qwen2.5:1.5b` (local) | ✅ | too weak — loses the thread |
| `mistral:7b` (local) | ❌ | writes pseudocode, ignores tools |

If a run says *"the model made no tool calls"*, the model is the problem, not
the spec. Ratchet recovers tool calls that a model emits as fenced text, which
helps mid-tier models but cannot compensate for a model with no tool training.

**`ratchet provider test <name>` tells you in five seconds whether your model is
usable.** Always run it before anything else.

---

## Supported languages

The agent edits files and runs commands, so any language works. `init` detects
the ecosystem and picks the test command:

| | Manifest | Test command |
|---|---|---|
| Rust | `Cargo.toml` | `cargo test` |
| Node / TypeScript | `package.json` | `npm`/`pnpm`/`yarn test` |
| Python | `pyproject.toml`, `setup.py` | `pytest` |
| Go | `go.mod` | `go test ./...` |
| Ruby | `Gemfile` | `bundle exec rspec` |
| Java | `pom.xml`, `build.gradle` | `mvn test`, `./gradlew test` |
| PHP | `composer.json` | `vendor/bin/phpunit` |
| Make | `Makefile` | `make test` |

For anything else, set it yourself — this is the escape hatch that makes an
unrecognised language or a custom harness work unchanged:

```toml
[project]
test_command = "./scripts/verify.sh"
```

---

## Configuration

Everything lives in `ratchet.toml`, written by `init` and safe to edit by hand.

```toml
[project]
name = "my-project"
test_command = "cargo test"

[providers.claude]
kind = "anthropic"
api_key_env = "ANTHROPIC_API_KEY"       # the env var name, never the key itself

[routing]
default = "claude"
policy = "capability-then-cost"

[sandbox]
allowed_paths = ["src", ".ratchet"]     # read and write
read_only_paths = [".agents"]           # read only
shell_allowlist = ["cargo test", "git diff"]
network_allowed = false
```

Credentials come from the environment or the OS keychain — nothing secret is
ever written to the repository. See **[docs/configuration.md](docs/configuration.md)**
for providers, skills, and the sandbox.

---

## Documentation

| | |
|---|---|
| **[Configuration](docs/configuration.md)** | `ratchet.toml`, providers, credentials, sandbox, skills, spec import |
| **[Extending](docs/extending.md)** | Multi-agent delegation, plugins, ACP, MCP, dashboard and A2A |
| **[How it works](docs/internals.md)** | Architecture, the spec-driven loop, metrics, development |
| **[Testing without a model](docs/testing.md)** | A scripted mock model, so the harness can be exercised on its own |

---

## Status

Working and tested (**207 tests**, CI green on Linux, macOS and Windows), but
young. It has been exercised end to end against a scripted model server and a
local Ollama model; the hosted provider adapters are written to the documented
APIs but have not been run against live endpoints in CI. See
[docs/internals.md](docs/internals.md) for details.

---

## License

MIT — see [LICENSE](LICENSE).
