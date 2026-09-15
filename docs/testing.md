# Trying Ratchet without a model

## The scripted mock model

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

---

See also: [How it works](internals.md) · [back to README](../README.md)
