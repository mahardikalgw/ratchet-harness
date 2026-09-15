# Extending Ratchet

Ratchet is driven by other programs as often as by a person. This covers
the integration surfaces: editors, external tools, custom gates, and
multi-agent delegation.

## Multi-agent delegation

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

---

## Plugins (custom tools and verification gates)

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

---

## Editor integration (ACP)

Start the ACP server and point any ACP-aware editor (Zed and others) at it:

```bash
ratchet serve --port 8765
```

Supported methods: `initialize`, `agent/run`, `agent/plan`, `agent/approve`, `agent/status`.

---

---

## MCP Integration

Inspect the tools and resources exposed by an MCP server:

```bash
ratchet mcp npx -y @modelcontextprotocol/server-filesystem /path/to/dir
```

The MCP client speaks JSON-RPC over stdio and supports `tools/list`, `tools/call`,
`resources/list`, and `resources/read`.

---

---

## Dashboard and A2A

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

---

See also: [Configuration](configuration.md) · [How it works](internals.md) · [back to README](../README.md)
