#!/usr/bin/env python3
"""A scripted, OpenAI-compatible model server for testing Ratchet end to end.

Why this exists
---------------
Ratchet's behaviour depends on the model it is driving. A weak local model, or
a rate-limited API, makes it hard to tell whether the *harness* is correct or
the *model* is. This server removes that variable: it behaves like a competent
tool-calling model that always does the same thing, so a failure means the
harness is wrong.

It implements just enough of the OpenAI Chat Completions API:

    GET  /v1/models
    POST /v1/chat/completions

Usage
-----
    python3 examples/mock-model-server.py --port 8899

    # simulate a reviewer that rejects the first attempt
    python3 examples/mock-model-server.py --port 8899 --reject-first-review

    # simulate a flaky endpoint so retry + failover can be exercised
    python3 examples/mock-model-server.py --port 8899 --fail-first 2

Then point a provider at it:

    [providers.mock]
    kind = "mimo"                       # any OpenAI-compatible adapter
    base_url = "http://127.0.0.1:8899/v1"
    model = "mock-model"
    api_key_env = "MOCK_API_KEY"        # value is ignored

Every request is logged to stderr, so you can see exactly what Ratchet sent.
"""

from __future__ import annotations

import argparse
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

# ---------------------------------------------------------------------------
# The "work" the mock model performs
# ---------------------------------------------------------------------------

CATALOG_RS = '''//! Toko online sederhana.

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A product in the catalogue.
#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub sku: String,
    pub name: String,
    pub price: u64,
}

/// Everything on sale, in display order.
pub fn catalog() -> Vec<Product> {
    vec![
        Product { sku: "SKU-1".into(), name: "Kopi Arabika".into(), price: 85000 },
        Product { sku: "SKU-2".into(), name: "Teh Hijau".into(), price: 45000 },
    ]
}

/// Total for a single-product checkout.
pub fn checkout_price(sku: &str) -> Option<u64> {
    catalog().into_iter().find(|p| p.sku == sku).map(|p| p.price)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_not_empty() {
        assert!(!catalog().is_empty());
    }

    #[test]
    fn finds_a_product_by_sku() {
        assert_eq!(checkout_price("SKU-1"), Some(85000));
    }

    #[test]
    fn unknown_sku_is_none() {
        assert_eq!(checkout_price("NOPE"), None);
    }
}
'''

PLAN = {
    "summary": "Add a product catalogue and single-product checkout.",
    "affected_modules": ["src/lib.rs"],
    "data_model_changes": ["Product struct"],
    "risk_notes": ["no persistence yet; the catalogue is hard-coded"],
    "tasks": [
        {
            "id": "T-1",
            "title": "Implement the catalogue",
            "description": "Add Product, catalog() and checkout_price() to src/lib.rs with tests.",
            "depends_on": [],
            "verification": {"kind": "test", "command": "cargo test"},
        }
    ],
}


DISCOVERY_QUESTIONS = {
    "done": False,
    "rationale": "Dua hal yang mengubah desainnya:",
    "questions": [
        {
            "id": "q1",
            "question": "Jual produk fisik, digital, atau keduanya?",
            "kind": "choice",
            "options": ["fisik", "digital", "keduanya"],
            "default": "fisik",
        },
        {
            "id": "q2",
            "question": "Payment gateway apa yang dipakai?",
            "kind": "text",
            "default": "midtrans",
        },
    ],
}

DISCOVERY_SPEC = {
    "done": True,
    "spec": {
        "id": "toko-online",
        "title": "Toko Online Sederhana",
        "goals": ["Menampilkan katalog produk", "Checkout satu produk"],
        "non_goals": ["Multi-vendor", "Manajemen gudang"],
        "acceptance_criteria": [
            {"id": "AC-1", "description": "Fungsi katalog tersedia", "verify_diff": "src/lib.rs"},
            {"id": "AC-2", "description": "Test lulus", "verify": "cargo test"},
        ],
        "constraints": ["Bahasa Indonesia"],
    },
}


class State:
    """Counters that let the server simulate not-always-perfect behaviour."""

    def __init__(self, args: argparse.Namespace) -> None:
        self.fail_first = args.fail_first
        self.reject_first_review = args.reject_first_review
        self.requests = 0
        self.reviews = 0
        self.discovery_round = 0

    def should_fail(self) -> bool:
        self.requests += 1
        return self.requests <= self.fail_first

    def verdict(self) -> dict:
        """Reject the first review, approve everything after."""
        self.reviews += 1
        if self.reject_first_review and self.reviews == 1:
            return {
                "approved": False,
                "issues": ["the empty-input case is not tested"],
                "summary": "needs another test before this can land",
            }
        return {"approved": True, "issues": [], "summary": "change looks correct"}


# ---------------------------------------------------------------------------
# Response shaping
# ---------------------------------------------------------------------------


def text_response(content: str) -> dict:
    return {
        "choices": [
            {"message": {"role": "assistant", "content": content}, "finish_reason": "stop"}
        ],
        "usage": {"prompt_tokens": 120, "completion_tokens": 40},
    }


def tool_response(call_id: str, name: str, arguments: dict) -> dict:
    return {
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": call_id,
                            "type": "function",
                            "function": {"name": name, "arguments": json.dumps(arguments)},
                        }
                    ],
                },
                "finish_reason": "tool_calls",
            }
        ],
        "usage": {"prompt_tokens": 220, "completion_tokens": 60},
    }


def decide(body: dict, state: State) -> dict:
    """Choose a response based on the conversation so far."""
    messages = body.get("messages", [])
    tools = body.get("tools") or []
    prompt = "\n".join(m.get("content") or "" for m in messages)
    roles = [m.get("role") for m in messages]

    # --- no tools offered: conversation, planning, or reviewing ----------
    if not tools:
        # Elicitation: ask once, then propose the spec.
        if "turning a request into a precise, verifiable specification" in prompt:
            state.discovery_round += 1
            payload = (
                DISCOVERY_QUESTIONS if state.discovery_round == 1 else DISCOVERY_SPEC
            )
            return text_response(json.dumps(payload))
        # Post-run narration.
        if "You just finished working on this request" in prompt:
            return text_response(
                "Selesai. Saya menambahkan fungsi katalog di src/lib.rs beserta "
                "test-nya, dan seluruh kriteria otomatis sudah lolos. "
                "Tidak ada yang keluar dari rencana."
            )
        if "Generate a technical plan" in prompt:
            return text_response(json.dumps(PLAN))
        if "Review the following" in prompt:
            return text_response(json.dumps(state.verdict()))
        if "rejected your previous attempt" in prompt:
            return text_response("Added the missing test and re-ran the suite.")
        return text_response("acknowledged")

    # --- tools offered: an implementation turn ---------------------------
    if "tool" in roles:
        # Tool results are present, so the work is done.
        return text_response(
            "Implemented slugify in src/lib.rs and confirmed the test suite passes."
        )

    available = {t["function"]["name"] for t in tools}
    if "file_write" in available:
        return tool_response(
            "call-1", "file_write", {"path": "src/lib.rs", "content": CATALOG_RS}
        )
    if "list_dir" in available:
        return tool_response("call-1", "list_dir", {"path": "."})
    return text_response("no suitable tool was offered")


# ---------------------------------------------------------------------------
# HTTP plumbing
# ---------------------------------------------------------------------------


class Handler(BaseHTTPRequestHandler):
    state: State

    def log_message(self, *args) -> None:  # silence the default access log
        pass

    def _send(self, status: int, payload: dict) -> None:
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self) -> None:
        if self.path.endswith("/models"):
            self._send(200, {"data": [{"id": "mock-model", "object": "model"}]})
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self) -> None:
        if not self.path.endswith("/chat/completions"):
            self.send_response(404)
            self.end_headers()
            return

        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            body = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            self._send(400, {"error": {"message": "invalid JSON"}})
            return

        # Simulate a flaky endpoint so retry/backoff and failover can be tested.
        if self.state.should_fail():
            print(
                f"[mock] injecting 503 failure (request {self.state.requests})",
                file=sys.stderr,
            )
            self._send(503, {"error": {"message": "service temporarily unavailable"}})
            return

        messages = body.get("messages", [])
        tools = body.get("tools") or []
        kind = "plan/review" if not tools else f"tools({len(tools)})"
        print(
            f"[mock] {kind} · {len(messages)} message(s) · "
            f"model={body.get('model')}",
            file=sys.stderr,
        )

        self._send(200, decide(body, self.state))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8899)
    parser.add_argument(
        "--fail-first",
        type=int,
        default=0,
        metavar="N",
        help="return 503 for the first N requests, to exercise retry/failover",
    )
    parser.add_argument(
        "--reject-first-review",
        action="store_true",
        help="have the reviewer reject the first attempt, to exercise revision rounds",
    )
    args = parser.parse_args()

    Handler.state = State(args)

    server = HTTPServer(("127.0.0.1", args.port), Handler)
    print(f"mock model server listening on http://127.0.0.1:{args.port}/v1", file=sys.stderr)
    print("  base_url = http://127.0.0.1:%d/v1" % args.port, file=sys.stderr)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
