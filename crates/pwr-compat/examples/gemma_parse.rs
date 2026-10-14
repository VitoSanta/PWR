//! Offline benchmark bridge: use the product Gemma reader, not a second parser.
use pwr_compat::{Gemma4Adapter, ModelBehaviorAdapter};
use pwr_provider::ModelReply;
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let result = (|| -> Result<serde_json::Value, String> {
            let input: serde_json::Value = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if input["op"] == "hash" {
                return Ok(
                    serde_json::json!({"hash": pwr_domain::hash_bytes(input["content"].as_str().unwrap_or_default())}),
                );
            }
            let reply = Gemma4Adapter.normalize(&ModelReply {
                content: input["content"].as_str().unwrap_or_default().into(),
                thinking: input["thinking"].as_str().unwrap_or_default().into(),
                ..Default::default()
            });
            Ok(
                serde_json::json!({"tool_calls": reply.tool_calls, "narrative": reply.narrative,
                "diagnostics": reply.diagnostics.iter().map(|d| serde_json::json!({"kind":d.kind,"detail":d.detail})).collect::<Vec<_>>()}),
            )
        })();
        println!(
            "{}",
            result.unwrap_or_else(|e| serde_json::json!({"error":e}))
        );
    }
}
