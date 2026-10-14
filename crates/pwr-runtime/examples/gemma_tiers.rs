//! Offline tier report using the product window arithmetic, no model load.
use pwr_runtime::window::{HostBudget, ModelShape, decide};
use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::args_os().nth(1).ok_or("supply the MLX model root")?);
    let mut rows = vec![];
    for (name, tiers) in [
        ("gemma-4-12B-it-4bit", vec![16u64]),
        ("gemma-4-26b-a4b-it-4bit", vec![24, 32]),
        ("gemma-4-26B-A4B-it-qat-4bit", vec![24, 32]),
        ("gemma-4-26b-a4b-it-6bit", vec![24, 32]),
        ("gemma-4-31b-it-8bit", vec![64]),
    ] {
        let dir = root.join(name);
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("config.json"))?)?;
        let weights: u64 = fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "safetensors"))
            .map(|e| e.metadata().map(|m| m.len()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum();
        let mut shape = ModelShape::from_config(&config);
        shape.weights_bytes = Some(weights);
        // Gemma keeps KV at full width. Show the best fused-attention bound
        // as well as the default conservative unknown-engine bound.
        for gb in tiers {
            let host = HostBudget::with_default_reserve(gb * 1024u64.pow(3));
            for mode in [
                "conservative_unknown_engine",
                "fused_attention",
                "layer_checkpoint_scores_1024mb",
            ] {
                shape.prefill_scores_bytes = match mode {
                    "fused_attention" => Some(0),
                    "layer_checkpoint_scores_1024mb" => Some(1536 * 1024 * 1024),
                    _ => None,
                };
                shape.checkpoint_by_layer =
                    (mode == "layer_checkpoint_scores_1024mb").then_some(true);
                let result = decide(&shape, Some(&host), None, Some(262144));
                rows.push(serde_json::json!({"model":name,"host_gib":gb,"weights_bytes":weights,
                    "kv_bytes_per_token":shape.kv_bytes_per_token,"kv_bits":16,"engine":"offline",
                    "assumption":mode,"reserve_bytes":host.reserve_bytes,
                    "target_tokens":262144,"decision":result.as_ref().ok(),"error":result.err().map(|e| e.to_string())}));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}
