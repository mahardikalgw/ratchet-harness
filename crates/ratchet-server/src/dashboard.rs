use serde_json::{Value, json};

/// Data the dashboard renders. Implemented by whoever owns the project state.
#[async_trait::async_trait]
pub trait DashboardSource: Send + Sync {
    /// Aggregate cost/token/outcome metrics.
    async fn summary(&self) -> Value;

    /// Recent per-task records.
    async fn tasks(&self) -> Value;

    /// Specs and their status.
    async fn specs(&self) -> Value;

    /// Render the dashboard page.
    async fn html(&self) -> String {
        let summary = self.summary().await;
        let tasks = self.tasks().await;
        let specs = self.specs().await;
        render_dashboard(&summary, &tasks, &specs)
    }
}

/// A dashboard with nothing behind it. Used when no project is loaded.
pub struct NullDashboard;

#[async_trait::async_trait]
impl DashboardSource for NullDashboard {
    async fn summary(&self) -> Value {
        json!({"total_tasks": 0, "note": "no project loaded"})
    }

    async fn tasks(&self) -> Value {
        json!([])
    }

    async fn specs(&self) -> Value {
        json!([])
    }
}

/// Self-contained dashboard: no external assets, no CDN, no build step.
pub fn render_dashboard(summary: &Value, tasks: &Value, specs: &Value) -> String {
    let total = summary
        .get("total_tasks")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cost = summary
        .get("total_estimated_cost_usd")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let passed = summary
        .get("tasks_passed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let failed = summary
        .get("tasks_failed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tokens_in = summary
        .get("total_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let tokens_out = summary
        .get("total_output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let spec_rows = rows_for(
        specs,
        &["id", "title", "status", "priority"],
        "No specs found.",
    );
    let task_rows = rows_for(
        tasks,
        &[
            "task_id",
            "provider",
            "model",
            "input_tokens",
            "output_tokens",
            "estimated_cost_usd",
        ],
        "No task records yet.",
    );

    let providers = summary
        .get("by_provider")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .map(|(name, agg)| {
                    let tasks = agg.get("tasks").and_then(|v| v.as_u64()).unwrap_or(0);
                    let cost = agg
                        .get("estimated_cost_usd")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    format!(
                        "<tr><td>{}</td><td>{}</td><td>${:.4}</td></tr>",
                        escape(name),
                        tasks,
                        cost
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "<tr><td colspan=3>none</td></tr>".to_string());

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ratchet</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{ font: 14px/1.5 ui-sans-serif, system-ui, -apple-system, sans-serif;
         margin: 0; padding: 2rem; max-width: 1100px; }}
  h1 {{ margin: 0 0 .25rem; font-size: 1.6rem; }}
  h2 {{ margin: 2rem 0 .5rem; font-size: 1.1rem; }}
  .sub {{ opacity: .65; margin-bottom: 2rem; }}
  .cards {{ display: grid; grid-template-columns: repeat(auto-fit,minmax(150px,1fr)); gap: .75rem; }}
  .card {{ border: 1px solid color-mix(in srgb, currentColor 20%, transparent);
           border-radius: 8px; padding: .75rem 1rem; }}
  .card .k {{ font-size: .75rem; opacity: .65; text-transform: uppercase; letter-spacing: .04em; }}
  .card .v {{ font-size: 1.4rem; font-variant-numeric: tabular-nums; }}
  table {{ border-collapse: collapse; width: 100%; }}
  th, td {{ text-align: left; padding: .4rem .6rem;
            border-bottom: 1px solid color-mix(in srgb, currentColor 12%, transparent); }}
  th {{ font-size: .75rem; text-transform: uppercase; letter-spacing: .04em; opacity: .65; }}
  code {{ font-family: ui-monospace, SFMono-Regular, monospace; }}
  footer {{ margin-top: 3rem; opacity: .5; font-size: .8rem; }}
</style>
</head>
<body>
  <h1>Ratchet</h1>
  <div class="sub">Spec-driven engineering harness — local team view</div>

  <div class="cards">
    <div class="card"><div class="k">Tasks</div><div class="v">{total}</div></div>
    <div class="card"><div class="k">Passed</div><div class="v">{passed}</div></div>
    <div class="card"><div class="k">Failed</div><div class="v">{failed}</div></div>
    <div class="card"><div class="k">Cost</div><div class="v">${cost:.4}</div></div>
    <div class="card"><div class="k">Tokens in</div><div class="v">{tokens_in}</div></div>
    <div class="card"><div class="k">Tokens out</div><div class="v">{tokens_out}</div></div>
  </div>

  <h2>By provider</h2>
  <table>
    <thead><tr><th>Provider</th><th>Tasks</th><th>Cost</th></tr></thead>
    <tbody>{providers}</tbody>
  </table>

  <h2>Specs</h2>
  <table>{spec_rows}</table>

  <h2>Recent tasks</h2>
  <table>{task_rows}</table>

  <footer>
    JSON endpoints: <code>/api/summary</code>, <code>/api/tasks</code>,
    <code>/api/specs</code> · A2A card: <code>/.well-known/agent.json</code>
  </footer>
</body>
</html>"#
    )
}

/// Turn a JSON array of objects into an HTML table body.
fn rows_for(value: &Value, columns: &[&str], empty: &str) -> String {
    let Some(items) = value.as_array() else {
        return format!("<tr><td>{}</td></tr>", escape(empty));
    };
    if items.is_empty() {
        return format!(
            "<tr><td colspan={}>{}</td></tr>",
            columns.len(),
            escape(empty)
        );
    }

    let header = format!(
        "<thead><tr>{}</tr></thead>",
        columns
            .iter()
            .map(|c| format!("<th>{}</th>", escape(c)))
            .collect::<Vec<_>>()
            .join("")
    );

    let body = items
        .iter()
        .take(50)
        .map(|item| {
            let cells = columns
                .iter()
                .map(|c| {
                    let raw = item.get(*c);
                    let text = match raw {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Null) | None => "—".to_string(),
                        Some(other) => other.to_string(),
                    };
                    format!("<td>{}</td>", escape(&text))
                })
                .collect::<Vec<_>>()
                .join("");
            format!("<tr>{cells}</tr>")
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("{header}<tbody>{body}</tbody>")
}

/// Escape text for embedding in HTML. Without this, a spec title containing
/// markup would be injected straight into the dashboard.
fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
