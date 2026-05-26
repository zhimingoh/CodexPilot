use serde_json::{Value, json};

pub(crate) fn install_bridge_script_template(binding_name: &str) -> String {
    format!(
        r#"
(() => {{
  window.__codexPilotCallbacks = new Map();
  window.__codexPilotSeq = 0;
  window.__codexPilotResolve = (id, result) => {{
    const callback = window.__codexPilotCallbacks.get(id);
    if (!callback) return;
    window.__codexPilotCallbacks.delete(id);
    callback.resolve(result);
  }};
  window.__codexPilotReject = (id, message) => {{
    const callback = window.__codexPilotCallbacks.get(id);
    if (!callback) return;
    window.__codexPilotCallbacks.delete(id);
    callback.resolve({{ status: "failed", message }});
  }};
  window.__codexPilotBridge = (path, payload) => new Promise((resolve) => {{
    const id = String(++window.__codexPilotSeq);
    window.__codexPilotCallbacks.set(id, {{ resolve }});
    window.{binding_name}(JSON.stringify({{ id, path, payload }}));
  }});
}})();
"#
    )
}

pub(crate) fn runtime_evaluate_params(script: &str) -> Value {
    json!({
        "expression": script,
        "awaitPromise": false,
        "allowUnsafeEvalBlockedByCSP": true,
    })
}

pub(crate) fn resolve_bridge_expression(
    request_id: &str,
    result: &Value,
) -> anyhow::Result<String> {
    Ok(format!(
        "window.__codexPilotResolve({}, {})",
        serde_json::to_string(request_id)?,
        serde_json::to_string(result)?
    ))
}

pub(crate) fn reject_bridge_expression(request_id: &str, message: &str) -> anyhow::Result<String> {
    Ok(format!(
        "window.__codexPilotReject({}, {})",
        serde_json::to_string(request_id)?,
        serde_json::to_string(message)?
    ))
}
