//! Conservative sampling recommendations from model cards.
//!
//! Cards are unstructured Markdown. Only assignments inside an explicitly
//! named sampling section are considered; prose, benchmark recipes and code
//! blocks are never fed to the agent or treated as instructions. An absent or
//! ambiguous recommendation leaves the artifact's generation_config in force.

use crate::hub::HubClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const CACHE_FILE: &str = ".pwr-card-sampling.json";
const USER_FILE: &str = ".pwr-user-sampling.json";
const FETCH_BUDGET: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardSampling {
    pub artifact_revision: String,
    pub source_repository: String,
    pub source_revision: String,
    pub values: BTreeMap<String, Value>,
}

/// Saved per installed model. Validation against the active engine happens
/// before writing; reading is strict so a damaged file cannot silently turn
/// into an empty profile.
pub fn user_overrides(model_dir: &Path) -> Result<BTreeMap<String, Value>, String> {
    let path = model_dir.join(USER_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    if bytes.len() > 16 * 1024 {
        return Err(format!("{} is too large", path.display()));
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn save_user_overrides(
    model_dir: &Path,
    values: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let path = model_dir.join(USER_FILE);
    let bytes = serde_json::to_vec_pretty(values).map_err(|error| error.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("model sampling profile is too large".into());
    }
    let staged = model_dir.join(format!("{USER_FILE}.{}.tmp", std::process::id()));
    std::fs::write(&staged, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&staged, &path).map_err(|error| error.to_string())
}

impl CardSampling {
    pub fn url(&self, hub: &HubClient) -> String {
        hub.file_url(&self.source_repository, &self.source_revision, "README.md")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    schema_version: u32,
    artifact_revision: String,
    recommendation: Option<CardSampling>,
    #[serde(default)]
    retry_after_unix: Option<i64>,
}

/// Resolve once for a PWR-downloaded model, then use the pinned local cache.
/// Network errors and malformed cards are non-fatal: the model stays usable
/// with its own generation_config. The five-second budget bounds first use.
pub async fn for_installed(
    hub: &HubClient,
    repository: &str,
    model_dir: &Path,
) -> Option<CardSampling> {
    if !crate::catalog::is_repository(repository) {
        return None;
    }
    let revision = std::fs::read_to_string(model_dir.join(crate::download::REVISION_FILE)).ok()?;
    let revision = revision.trim();
    if !crate::catalog::is_revision(revision) {
        return None;
    }
    let cache_path = model_dir.join(CACHE_FILE);
    if let Ok(bytes) = std::fs::read(&cache_path)
        && bytes.len() <= 16 * 1024
        && let Ok(cache) = serde_json::from_slice::<Cached>(&bytes)
        && cache.schema_version == 1
        && cache.artifact_revision == revision
    {
        if cache
            .retry_after_unix
            .is_none_or(|retry| chrono::Utc::now().timestamp() < retry)
        {
            return cache.recommendation;
        }
    }
    let found = tokio::time::timeout(FETCH_BUDGET, fetch(hub, repository, revision)).await;
    let (recommendation, retry_after_unix) = match found {
        Ok(Some(recommendation)) => (recommendation, None),
        _ => (None, Some(chrono::Utc::now().timestamp() + 60)),
    };
    let cache = Cached {
        schema_version: 1,
        artifact_revision: revision.into(),
        recommendation: recommendation.clone(),
        retry_after_unix,
    };
    if let Ok(bytes) = serde_json::to_vec_pretty(&cache) {
        // Best effort: read-only model folders still work from the artifact.
        let _ = std::fs::write(cache_path, bytes);
    }
    recommendation
}

async fn fetch(hub: &HubClient, repository: &str, revision: &str) -> Option<Option<CardSampling>> {
    let exact = hub.card(repository, revision).await.ok()?;
    if let Some(values) = exact.as_deref().and_then(parse_recommendations) {
        return Some(Some(CardSampling {
            artifact_revision: revision.into(),
            source_repository: repository.into(),
            source_revision: revision.into(),
            values,
        }));
    }
    // A quantization card often names the original model but omits its
    // sampling section. Only trust that relationship when the current Hub
    // listing still names the downloaded commit.
    let model = hub.model(repository).await.ok()?;
    if model.revision.as_deref() != Some(revision) {
        return Some(None);
    }
    for base in model.base_models {
        if !crate::catalog::is_repository(&base) {
            continue;
        }
        let Ok(source) = hub.model(&base).await else {
            continue;
        };
        let Some(source_revision) = source.revision else {
            continue;
        };
        let Ok(Some(card)) = hub.card(&base, &source_revision).await else {
            continue;
        };
        if let Some(values) = parse_recommendations(&card) {
            return Some(Some(CardSampling {
                artifact_revision: revision.into(),
                source_repository: base,
                source_revision,
                values,
            }));
        }
    }
    Some(None)
}

/// Only literal `name=value` pairs in a section explicitly about sampling.
/// General-use lines take precedence over benchmark reproduction lines.
pub fn parse_recommendations(card: &str) -> Option<BTreeMap<String, Value>> {
    let mut section_level = None;
    let mut in_fence = false;
    let mut ordinary = BTreeMap::new();
    let mut general = BTreeMap::new();
    for line in card.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        let level = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        if let Some(start_level) = section_level {
            if level > 0 && level <= start_level {
                break;
            }
        } else if lower.contains("recommended sampling parameters")
            || (level > 0 && lower.contains("sampling parameters"))
        {
            section_level = Some(if level == 0 { 3 } else { level });
        } else {
            continue;
        }
        if lower.contains("benchmark") || lower.contains("reproduc") || lower.contains("evaluat") {
            continue;
        }
        let target = if lower.contains("general tasks") {
            &mut general
        } else {
            &mut ordinary
        };
        for token in trimmed
            .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '=')))
        {
            let Some((name, raw)) = token.split_once('=') else {
                continue;
            };
            let name = name.replace('-', "_").to_ascii_lowercase();
            let value = match name.as_str() {
                "temperature" => raw
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .map(|n| serde_json::json!(n)),
                "top_p" | "min_p" => raw
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                    .map(|n| serde_json::json!(n)),
                "top_k" => raw.parse::<u32>().ok().map(|n| serde_json::json!(n)),
                _ => None,
            };
            if let Some(value) = value {
                match target.get(&name) {
                    Some(previous) if previous != &value => return None,
                    _ => {
                        target.insert(name, value);
                    }
                }
            }
        }
    }
    if !general.is_empty() {
        Some(general)
    } else if !ordinary.is_empty() {
        Some(ordinary)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ornith_general_use_beats_benchmark_recipe() {
        let card = "Recommended sampling parameters:\n\n* For general tasks: `temperature=0.6`, `top_p=0.95`, `top_k=20`\n* To reproduce the reported benchmarks: `temperature=1.0`\n### Serving\n";
        let found = parse_recommendations(card).unwrap();
        assert_eq!(found["temperature"], serde_json::json!(0.6));
        assert_eq!(found["top_p"], serde_json::json!(0.95));
        assert_eq!(found["top_k"], serde_json::json!(20));
    }

    #[test]
    fn gemma_best_practices_are_read_without_thinking_section() {
        let card = "## Best Practices\n### 1. Sampling Parameters\nUse this across all use cases:\n* `temperature=1.0`\n* `top_p=0.95`\n* `top_k=64`\n### 2. Thinking Mode Configuration\n* `temperature=0.2`\n";
        let found = parse_recommendations(card).unwrap();
        assert_eq!(found["temperature"], serde_json::json!(1.0));
        assert_eq!(found["top_p"], serde_json::json!(0.95));
        assert_eq!(found["top_k"], serde_json::json!(64));
    }

    #[test]
    fn benchmarks_and_code_without_a_sampling_section_are_not_recommendations() {
        assert!(parse_recommendations("Benchmark temp=1.0 top_p=1.0").is_none());
        assert!(parse_recommendations("```python\ntemperature=1.0\n```").is_none());
    }

    #[test]
    fn user_overrides_replace_and_reset_without_touching_the_model() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("generation_config.json"), "original").unwrap();
        assert!(user_overrides(dir.path()).unwrap().is_empty());
        let values = BTreeMap::from([("temperature".into(), serde_json::json!(0.7))]);
        save_user_overrides(dir.path(), &values).unwrap();
        assert_eq!(user_overrides(dir.path()).unwrap(), values);
        save_user_overrides(dir.path(), &BTreeMap::new()).unwrap();
        assert!(user_overrides(dir.path()).unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("generation_config.json")).unwrap(),
            "original"
        );
    }
}
