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
    /// The file the values were read from: the card (`README.md`, the
    /// default) or the original model's `generation_config.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
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
        let file = self.source_file.as_deref().unwrap_or("README.md");
        hub.file_url(&self.source_repository, &self.source_revision, file)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    schema_version: u32,
    artifact_revision: String,
    repository: String,
    mode: SamplingMode,
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
    for_installed_for_mode(hub, repository, model_dir, SamplingMode::Unknown).await
}

/// Only use a known mode when it is fixed for every request that will consume
/// the result. Chat/eval enrichment intentionally uses the unknown-mode wrapper.
pub async fn for_installed_for_mode(
    hub: &HubClient,
    repository: &str,
    model_dir: &Path,
    mode: SamplingMode,
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
        && cache.schema_version == 4
        && cache.repository == repository
        && cache.mode == mode
        && cache.artifact_revision == revision
        && cache.recommendation.as_ref().is_none_or(|card| {
            card.artifact_revision == revision
                && crate::catalog::is_repository(&card.source_repository)
                && crate::catalog::is_revision(&card.source_revision)
        })
        && cache
            .retry_after_unix
            .is_none_or(|retry| chrono::Utc::now().timestamp() < retry)
    {
        return cache.recommendation;
    }
    let found = tokio::time::timeout(FETCH_BUDGET, fetch(hub, repository, revision, mode)).await;
    let (recommendation, retry_after_unix) = match found {
        // A card that says nothing today may say something after an edit:
        // "none found" is looked for again after a day.
        Ok(Some(None)) => (None, Some(chrono::Utc::now().timestamp() + 24 * 3600)),
        Ok(Some(recommendation)) => (recommendation, None),
        _ => (None, Some(chrono::Utc::now().timestamp() + 60)),
    };
    let cache = Cached {
        // 4: reject the former unscoped/assumed-thinking selection; identity
        // includes repository and requested mode as well as artifact revision.
        schema_version: 4,
        repository: repository.into(),
        mode,
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

async fn fetch(
    hub: &HubClient,
    repository: &str,
    revision: &str,
    mode: SamplingMode,
) -> Option<Option<CardSampling>> {
    let exact = hub.card(repository, revision).await.ok()?;
    if let Some(values) = exact
        .as_deref()
        .and_then(|card| parse_recommendations_for_mode(card, mode))
    {
        return Some(Some(CardSampling {
            artifact_revision: revision.into(),
            source_repository: repository.into(),
            source_revision: revision.into(),
            values,
            source_file: None,
        }));
    }
    // A quantization card often names the original model but omits its
    // sampling section. Only trust that relationship when the current Hub
    // listing still names the downloaded commit.
    let model = hub.model(repository).await.ok()?;
    if model.revision.as_deref() != Some(revision) {
        return Some(None);
    }
    for base in &model.base_models {
        if !crate::catalog::is_repository(base) {
            continue;
        }
        let Ok(source) = hub.model(base).await else {
            continue;
        };
        let Some(source_revision) = source.revision else {
            continue;
        };
        let Ok(Some(card)) = hub.card(base, &source_revision).await else {
            continue;
        };
        if let Some(values) = parse_recommendations_for_mode(&card, mode) {
            return Some(Some(CardSampling {
                artifact_revision: revision.into(),
                source_repository: base.clone(),
                source_revision,
                values,
                source_file: None,
            }));
        }
    }
    // No card says, so the original model's own generation_config.json,
    // which a conversion often leaves out. Measured 2026-09-29: the
    // lmstudio-community MLX builds of Qwen2.5-Coder-14B and Qwen3-14B ship
    // none, so they ran at temperature 0 -- greedy decoding, which Qwen's
    // card for Qwen3 warns leads to endless repetition -- while
    // Qwen/Qwen3-14B's says 0.6, top_p 0.95, top_k 20.
    for base in &model.base_models {
        if !crate::catalog::is_repository(base) {
            continue;
        }
        let Ok(source) = hub.model(base).await else {
            continue;
        };
        let Some(source_revision) = source.revision else {
            continue;
        };
        let Ok(Some(config)) = hub.generation_config(base, &source_revision).await else {
            continue;
        };
        if let Some(values) = generation_sampling(&config) {
            return Some(Some(CardSampling {
                artifact_revision: revision.into(),
                source_repository: base.clone(),
                source_revision,
                values,
                source_file: Some("generation_config.json".into()),
            }));
        }
    }
    Some(None)
}

/// The sampling a `generation_config.json` sets, when it samples at all: a
/// config with `do_sample: false` asks for greedy decoding, which is what an
/// absent one already gives.
pub fn generation_sampling(config: &Value) -> Option<BTreeMap<String, Value>> {
    if config.get("do_sample").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let values: BTreeMap<String, Value> = [
        "temperature",
        "top_p",
        "top_k",
        "min_p",
        "repetition_penalty",
    ]
    .into_iter()
    .filter_map(|name| {
        let value = config.get(name)?;
        value.is_number().then(|| (name.to_owned(), value.clone()))
    })
    .collect();
    // `do_sample: true` with no temperature samples at 1.0, the Hub's
    // default -- and what OpenAI recommends for gpt-oss, whose config says
    // only that. Left empty, it ran greedy instead.
    let mut values = values;
    if config.get("do_sample").and_then(Value::as_bool) == Some(true) {
        values
            .entry("temperature".into())
            .or_insert(serde_json::json!(1.0));
    }
    (!values.is_empty()).then_some(values)
}

/// A requested template switch, not a capability or an inferred default.
/// Callers whose planner can change the switch must use `Unknown`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SamplingMode {
    #[default]
    Unknown,
    Thinking,
    NonThinking,
}

/// Existing chat/eval callers resolve before per-generation reasoning planning,
/// so only mode-neutral recommendations are safe at this boundary.
pub fn parse_recommendations(card: &str) -> Option<BTreeMap<String, Value>> {
    parse_recommendations_for_mode(card, SamplingMode::Unknown)
}

#[derive(Clone, Copy, Default)]
struct RecipeContext {
    excluded: bool,
    mode: SamplingMode,
    // Within a known mode prefer coding, then general; neutral cards retain
    // their explicit general-use precedence over task-specific recipes.
    task: u8,
    recipe: usize,
}

fn excluded_label(lower: &str) -> bool {
    lower.contains("benchmark") || lower.contains("reproduc") || lower.contains("evaluat")
}

fn labelled_context(lower: &str, mut parent: RecipeContext, recipe: usize) -> RecipeContext {
    parent.excluded |= excluded_label(lower);
    // Non-thinking also contains "thinking"; classify it first.
    if lower.contains("non-thinking")
        || lower.contains("non thinking")
        || lower.contains("instruct mode")
    {
        parent.mode = SamplingMode::NonThinking;
    } else if lower.contains("thinking") {
        parent.mode = SamplingMode::Thinking;
    }
    if lower.contains("coding") {
        parent.task = 2;
    } else if lower.contains("general tasks") {
        parent.task = 1;
    }
    if lower.contains("thinking")
        || lower.contains("non-thinking")
        || lower.contains("non thinking")
        || lower.contains("instruct mode")
        || lower.contains("coding")
        || lower.contains("general tasks")
        || excluded_label(lower)
    {
        parent.recipe = recipe;
    }
    parent
}

/// Literal assignments inside a named sampling scope. This deliberately
/// supports a small subset of Markdown/HTML rather than guessing from prose.
/// Unknown mode accepts no mode-specific recipe. Conflicting recipes at the
/// selected priority are absent; partial alternatives are never blended.
pub fn parse_recommendations_for_mode(
    card: &str,
    mode: SamplingMode,
) -> Option<BTreeMap<String, Value>> {
    let mut headings: Vec<(usize, RecipeContext)> = Vec::new();
    let mut items: Vec<(usize, RecipeContext)> = Vec::new();
    // heading boundary, numbered-list boundary, quoted scope
    let mut section: Option<(usize, Option<usize>, bool)> = None;
    let mut fence: Option<(char, usize)> = None;
    let mut html_code = false;
    let mut recipes: BTreeMap<usize, (RecipeContext, BTreeMap<String, Value>, bool)> =
        BTreeMap::new();
    let mut serial = 0;
    let mut root = RecipeContext::default();
    let mut paragraph = None;
    for original in card.lines() {
        let mut line = original.trim_start();
        let quoted = line.starts_with('>');
        while let Some(rest) = line.strip_prefix('>') {
            line = rest.strip_prefix(' ').unwrap_or(rest);
        }
        let indentation = if quoted { line } else { original };
        let indent = indentation
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum::<usize>();
        let trimmed = line.trim();
        let delimiter = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        if let Some(ch) = delimiter {
            let count = trimmed.chars().take_while(|c| *c == ch).count();
            if count >= 3 {
                match fence {
                    None => fence = Some((ch, count)),
                    Some((open, length))
                        if ch == open && count >= length && trimmed[count..].trim().is_empty() =>
                    {
                        fence = None
                    }
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if html_code || lower.contains("<pre") || lower.contains("<script") {
            html_code = !(lower.contains("</pre>") || lower.contains("</script>"));
            continue;
        }
        let level = trimmed.bytes().take_while(|b| *b == b'#').count();
        let numbered = trimmed.split_once('.').is_some_and(|(number, rest)| {
            !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && rest.starts_with(' ')
        });
        let item = numbered
            || ["- ", "* ", "+ "]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix));
        if section.is_some_and(|(start, list, was_quoted)| {
            (level > 0 && level <= start)
                || (numbered && list.is_some_and(|start| indent <= start))
                || (was_quoted && !quoted)
                || lower == "</ul>"
        }) {
            section = None;
            root = RecipeContext::default();
            paragraph = None;
            items.clear();
        }
        serial += 1;
        if level > 0 {
            while headings.last().is_some_and(|(at, _)| *at >= level) {
                headings.pop();
            }
            items.clear();
            paragraph = None;
        } else if item {
            while items.last().is_some_and(|(at, _)| *at >= indent) {
                items.pop();
            }
        } else {
            while items.last().is_some_and(|(at, _)| *at > indent) {
                items.pop();
            }
        }
        let parent = items
            .last()
            .map(|(_, ctx)| *ctx)
            .or(if level == 0 { paragraph } else { None })
            .or_else(|| headings.last().map(|(_, ctx)| *ctx))
            .unwrap_or(root);
        let mut context = labelled_context(&lower, parent, serial);
        let scope_label = lower.contains("sampling parameters")
            && (level > 0
                || lower.contains("recommended sampling parameters")
                || lower.contains("recommend using")
                || (numbered && lower.contains("**sampling parameters**")));
        if scope_label && !context.excluded {
            section = Some((
                if level > 0 { level } else { usize::MAX },
                numbered.then_some(indent),
                quoted,
            ));
            context.recipe = serial;
            root = context;
        }
        if level > 0 {
            headings.push((level, context));
        }
        if item {
            items.push((indent, context));
        }
        // An unstructured label can introduce following lines, including
        // bullets. Excluded labels must persist too, before any early return.
        if context.recipe == serial && !item && level == 0 && !lower.contains("<li") {
            paragraph = Some(context);
        }
        // Markdown code is indented four columns relative to list content.
        // Supported list continuation has at most three extra columns after
        // its marker; plain code has four or more.
        let code_indent = items.last().map_or(4, |(at, _)| at + 6);
        if indent >= code_indent || section.is_none() || context.excluded {
            continue;
        }
        let pairs = line_pairs(trimmed);
        if pairs.is_empty() {
            continue;
        }
        let (_, values, ambiguous) = recipes
            .entry(context.recipe)
            .or_insert_with(|| (context, BTreeMap::new(), false));
        for (name, value) in pairs {
            if values.get(&name).is_some_and(|old| old != &value) {
                *ambiguous = true;
            }
            values.insert(name, value);
        }
    }
    let priority = |ctx: RecipeContext| -> Option<u8> {
        if ctx.mode == SamplingMode::Unknown {
            Some(match ctx.task {
                1 => 2,
                2 => 0,
                _ => 1,
            })
        } else if mode != SamplingMode::Unknown && ctx.mode == mode {
            Some(3 + ctx.task)
        } else {
            None
        }
    };
    let best = recipes
        .values()
        .filter_map(|(ctx, _, _)| priority(*ctx))
        .max()?;
    let mut selected = None;
    for (ctx, values, ambiguous) in recipes.values() {
        if priority(*ctx) != Some(best) {
            continue;
        }
        if *ambiguous || selected.as_ref().is_some_and(|old| old != values) {
            return None;
        }
        selected = Some(values.clone());
    }
    selected
}

/// The literal `name=value` pairs on one line, each checked against the range
/// the engine accepts.
fn line_pairs(line: &str) -> Vec<(String, Value)> {
    let mut found = Vec::new();
    for token in
        line.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '=')))
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
            "top_k" => raw
                .parse::<u32>()
                .ok()
                .filter(|n| *n <= i32::MAX as u32)
                .map(|n| serde_json::json!(n)),
            // Qwen 3.5-family cards (Ornith's among them) recommend a
            // presence penalty against repetition; the engine takes both.
            "presence_penalty" => raw
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && (-2.0..=2.0).contains(n))
                .map(|n| serde_json::json!(n)),
            "repetition_penalty" => raw
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n > 0.0)
                .map(|n| serde_json::json!(n)),
            _ => None,
        };
        if let Some(value) = value {
            found.push((name, value));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_html_card_gives_its_general_sampling() {
        // Ornith-1.5-9B-MLX-4bit's card, as the Hub serves it: HTML, with
        // style attributes around every value.
        let code = |value: &str| {
            format!(
                "<code style=\"background:rgba(253,142,91,0.15);padding:1px 5px;border-radius:4px\">{value}</code>"
            )
        };
        let card = format!(
            "<p style=\"margin:0 0 6px\">Recommended sampling parameters:</p>\n<ul style=\"margin:0;padding-left:20px\">\n<li><b>For general tasks:</b> {}, {}, {}, {}, {}, {}</li>\n<li><b>For precise coding tasks:</b> {}, {}, {}</li>\n</ul>\n</div>\n\n### Serving Ornith-1.5-9B\n",
            code("temperature=1.0"),
            code("top_p=0.95"),
            code("top_k=20"),
            code("min_p=0.0"),
            code("presence_penalty=1.5"),
            code("repetition_penalty=1.0"),
            code("temperature=0.6"),
            code("top_p=0.95"),
            code("top_k=20"),
        );
        let values = super::parse_recommendations(&card).unwrap();
        assert_eq!(values["temperature"], 1.0, "{values:?}");
        assert_eq!(values["top_p"], 0.95);
        assert_eq!(values["top_k"], 20);
        assert_eq!(values["presence_penalty"], 1.5);
        assert!(!values.contains_key("style"), "{values:?}");
    }

    #[test]
    fn an_original_models_generation_config_gives_its_sampling() {
        // Qwen/Qwen3-14B's, as the Hub serves it.
        let qwen3 = serde_json::json!({"do_sample": true, "temperature": 0.6, "top_k": 20, "top_p": 0.95, "eos_token_id": [151645]});
        let values = super::generation_sampling(&qwen3).unwrap();
        assert_eq!(values["temperature"], 0.6);
        assert_eq!(values["top_k"], 20);
        assert!(!values.contains_key("eos_token_id"));
        // Greedy on purpose, or nothing about sampling: nothing to take.
        assert!(
            super::generation_sampling(
                &serde_json::json!({"do_sample": false, "temperature": 0.7})
            )
            .is_none()
        );
        assert!(super::generation_sampling(&serde_json::json!({"bos_token_id": 1})).is_none());
        // openai/gpt-oss-20b's says only that it samples: at 1.0.
        let gpt_oss = super::generation_sampling(
            &serde_json::json!({"do_sample": true, "eos_token_id": [200002]}),
        )
        .unwrap();
        assert_eq!(gpt_oss["temperature"], 1.0);
    }

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
    fn a_card_with_a_set_per_mode_gives_its_thinking_coding_set() {
        // Qwen3.5-9B's card, both layouts it uses.
        let tip = "> We recommend using the following set of sampling parameters for generation\n> - Thinking mode for general tasks: `temperature=1.0, top_p=0.95, top_k=20, presence_penalty=1.5`\n\
> - Thinking mode for precise coding tasks (e.g. WebDev): `temperature=0.6, top_p=0.95, top_k=20, min_p=0.0, presence_penalty=0.0, repetition_penalty=1.0`\n\
> - Instruct (or non-thinking) mode for general tasks: `temperature=0.7, top_p=0.8, top_k=20`";
        let found = parse_recommendations_for_mode(tip, SamplingMode::Thinking).unwrap();
        assert_eq!(found["temperature"], serde_json::json!(0.6));
        assert_eq!(found["top_k"], serde_json::json!(20));
        assert_eq!(found["presence_penalty"], serde_json::json!(0.0));
        let list = "1. **Sampling Parameters**:\n     - **Thinking mode for precise coding tasks (e.g., WebDev)**:  \n       `temperature=0.6`, `top_p=0.95`, `top_k=20`\n     - **Instruct mode for general tasks**:  \n       `temperature=0.7`, `top_p=0.8`";
        assert_eq!(
            parse_recommendations_for_mode(list, SamplingMode::Thinking).unwrap()["temperature"],
            serde_json::json!(0.6)
        );
    }

    #[test]
    fn conflicting_coding_sets_are_not_guessed() {
        let card = "- coding, thinking: temperature=0.6\n- coding, thinking: temperature=0.8";
        assert!(parse_recommendations(card).is_none());
        assert!(
            parse_recommendations(
                "coding tasks are fun, temperature=0.6 in a fence:\n```\ntemperature=1\n```"
            )
            .is_none()
        );
    }

    #[test]
    fn benchmarks_and_code_without_a_sampling_section_are_not_recommendations() {
        assert!(parse_recommendations("Benchmark temp=1.0 top_p=1.0").is_none());
        assert!(parse_recommendations("```python\ntemperature=1.0\n```").is_none());
    }

    #[test]
    fn unscoped_coding_prose_and_benchmarks_are_not_recommendations() {
        for card in [
            "Benchmark coding: temperature=1.0",
            "For coding use temperature=0.6",
            "Thinking coding:\n```temperature=0.6\n```",
        ] {
            assert!(parse_recommendations(card).is_none(), "{card}");
        }
    }

    #[test]
    fn unknown_mode_does_not_select_a_mode_recipe() {
        for card in [
            "## Sampling Parameters\nThinking mode for coding: temperature=0.6",
            "## Sampling Parameters\nNon-thinking mode: temperature=0.7",
            "## Sampling Parameters\n### Thinking mode\ntemperature=0.6",
        ] {
            assert!(parse_recommendations(card).is_none(), "{card}");
        }
    }

    #[test]
    fn benchmark_ancestry_excludes_nested_sampling_and_continuations() {
        for card in [
            "## Benchmarks\n### Sampling Parameters\ntemperature=1.0",
            "## Sampling Parameters\n### Benchmark reproduction\n- Coding\n  temperature=1.0",
            "## Sampling Parameters\n- Evaluation recipe:\n  temperature=1.0",
        ] {
            assert!(parse_recommendations(card).is_none(), "{card}");
        }
    }

    #[test]
    fn quoted_fences_and_mismatched_delimiters_do_not_expose_values() {
        for card in [
            "> Recommended sampling parameters:\n> ```\n> temperature=1.0\n> ```",
            "## Sampling Parameters\n```\n~~~\ntemperature=1.0\n```",
        ] {
            assert!(parse_recommendations(card).is_none(), "{card}");
        }
    }

    #[test]
    fn top_k_outside_engine_range_is_not_a_recommendation() {
        assert!(parse_recommendations("## Sampling Parameters\ntop_k=4294967295").is_none());
    }

    #[test]
    fn mode_headings_are_not_neutral_assignments() {
        assert!(
            parse_recommendations("## Sampling Parameters\n### Thinking\ntemperature=0.6")
                .is_none()
        );
    }

    #[test]
    fn explicit_mode_selects_only_the_matching_complete_recipe() {
        let card = "## Sampling Parameters\n### Thinking mode for general tasks\ntemperature=1.0\n### Thinking mode for coding\ntemperature=0.6\ntop_k=20\n### Instruct (non-thinking) mode\ntemperature=0.7\ntop_p=0.8";
        assert!(parse_recommendations(card).is_none());
        let thinking = parse_recommendations_for_mode(card, SamplingMode::Thinking).unwrap();
        assert_eq!(
            thinking,
            BTreeMap::from([
                ("temperature".into(), serde_json::json!(0.6)),
                ("top_k".into(), serde_json::json!(20))
            ])
        );
        let off = parse_recommendations_for_mode(card, SamplingMode::NonThinking).unwrap();
        assert_eq!(off["temperature"], 0.7);
        assert_eq!(off["top_p"], 0.8);
        assert!(!off.contains_key("top_k"));
    }

    #[test]
    fn alternate_partial_recipes_are_never_blended() {
        let card = "## Sampling Parameters\n- Thinking mode for coding: temperature=0.6, top_k=20\n- Thinking mode for coding: temperature=0.6, top_p=0.95";
        assert!(parse_recommendations_for_mode(card, SamplingMode::Thinking).is_none());
    }

    #[test]
    fn excluded_descendants_do_not_poison_a_neutral_recipe() {
        let card = "## Sampling Parameters\ntemperature=0.6\n### Benchmark reproduction\ntemperature=1.0\n#### Coding parameters\ntop_p=1.0\n## Serving\ntemperature=0.1";
        assert_eq!(
            parse_recommendations(card).unwrap(),
            BTreeMap::from([("temperature".into(), serde_json::json!(0.6))])
        );
    }

    #[test]
    fn labelled_scope_ends_before_sibling_or_unquoted_examples() {
        for card in [
            "1. **Sampling Parameters**:\n   - Thinking mode for coding:\n     ```temperature=0.6\n     ```\n2. **Coding example**:\n   temperature=1.0",
            "> Recommended sampling parameters:\n> No recommendation\ncoding temperature=1.0",
            "Recommended sampling parameters:\n<ul>\n</ul>\ncoding temperature=1.0",
            "## Sampling Parameters\n    temperature=1.0",
        ] {
            assert!(
                parse_recommendations_for_mode(card, SamplingMode::Thinking).is_none(),
                "{card}"
            );
        }
    }

    #[test]
    fn plain_labels_preserve_exclusion_and_mode_on_continuations() {
        for card in [
            "## Sampling Parameters\nBenchmark recipe:\ntemperature=1.0",
            "## Sampling Parameters\nThinking mode:\n- temperature=0.6",
            "## Sampling Parameters\nThinking mode:\n\ntemperature=0.6",
            "## Sampling Parameters\nBenchmark recipe:\n\ntemperature=1.0",
        ] {
            assert!(parse_recommendations(card).is_none(), "{card}");
        }
    }

    #[test]
    fn plain_sampling_label_stops_at_the_next_heading() {
        assert!(
            parse_recommendations("Recommended sampling parameters:\n\n## Usage\ntemperature=0.9")
                .is_none()
        );
    }

    #[test]
    fn list_indented_code_is_not_a_recommendation() {
        assert!(
            parse_recommendations(
                "## Sampling Parameters\n- Usage example:\n\n      temperature=1.0"
            )
            .is_none()
        );
    }

    #[test]
    fn html_and_tab_indented_code_are_excluded() {
        for card in [
            "## Sampling Parameters\n<pre><code>temperature=1.0</code></pre>",
            "## Sampling Parameters\n<pre>\ntemperature=1.0\n</pre>",
            "## Sampling Parameters\n\ttemperature=1.0",
        ] {
            assert!(parse_recommendations(card).is_none(), "{card}");
        }
    }

    const TEST_REVISION: &str = "1111111111111111111111111111111111111111";

    fn cached_model(schema: u32, repository: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(crate::download::REVISION_FILE),
            TEST_REVISION,
        )
        .unwrap();
        let cache = serde_json::json!({
            "schema_version": schema, "repository": repository, "mode": "unknown",
            "artifact_revision": TEST_REVISION, "retry_after_unix": null,
            "recommendation": {
                "artifact_revision": TEST_REVISION, "source_repository": "owner/source",
                "source_revision": TEST_REVISION, "values": {"temperature": 0.6},
                "source_file": "generation_config.json"
            }
        });
        std::fs::write(
            dir.path().join(CACHE_FILE),
            serde_json::to_vec(&cache).unwrap(),
        )
        .unwrap();
        dir
    }

    fn offline_hub() -> HubClient {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        HubClient::new(&format!("http://{address}"), None).unwrap()
    }

    #[tokio::test]
    async fn legacy_chosen_recipe_is_not_reused() {
        let dir = cached_model(3, "owner/model");
        assert!(
            for_installed(&offline_hub(), "owner/model", dir.path())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn cache_identity_includes_artifact_repository() {
        let dir = cached_model(4, "owner/other-model");
        assert!(
            for_installed(&offline_hub(), "owner/model", dir.path())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn pinned_cache_is_reused_only_for_its_mode_and_revision() {
        let dir = cached_model(4, "owner/model");
        let hub = offline_hub();
        let card = for_installed(&hub, "owner/model", dir.path())
            .await
            .unwrap();
        assert_eq!(
            card.url(&hub),
            hub.file_url("owner/source", TEST_REVISION, "generation_config.json")
        );
        assert!(
            for_installed_for_mode(&hub, "owner/model", dir.path(), SamplingMode::Thinking)
                .await
                .is_none()
        );
        let dir = cached_model(4, "owner/model");
        std::fs::write(
            dir.path().join(crate::download::REVISION_FILE),
            "2222222222222222222222222222222222222222",
        )
        .unwrap();
        assert!(
            for_installed(&hub, "owner/model", dir.path())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn expired_negative_cache_is_retried_and_malformed_provenance_rejected() {
        let hub = offline_hub();
        let dir = cached_model(4, "owner/model");
        let path = dir.path().join(CACHE_FILE);
        let mut cache: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        cache["recommendation"] = Value::Null;
        cache["retry_after_unix"] = serde_json::json!(chrono::Utc::now().timestamp() - 1);
        std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
        assert!(
            for_installed(&hub, "owner/model", dir.path())
                .await
                .is_none()
        );
        let rewritten: Cached = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(rewritten.retry_after_unix.unwrap() > chrono::Utc::now().timestamp());
        let dir = cached_model(4, "owner/model");
        let path = dir.path().join(CACHE_FILE);
        let mut cache: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        cache["recommendation"]["source_revision"] = serde_json::json!("main");
        std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
        assert!(
            for_installed(&hub, "owner/model", dir.path())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn installed_dynamic_caller_never_reuses_a_fixed_mode_recipe() {
        use std::io::{Read, Write};
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let address = socket.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let mut paths = Vec::new();
            while paths.len() < 4 && std::time::Instant::now() < deadline {
                let (mut stream, _) = match socket.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                // On macOS a socket accepted from a non-blocking listener is
                // non-blocking too, and a read before the request has arrived
                // fails with WouldBlock whatever the read timeout says: on a
                // hosted runner it did (CI run 37513133681), on a fast local
                // machine the bytes were already there.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..count]);
                let path = request.split_whitespace().nth(1).unwrap().to_owned();
                let (status, body) = if path.ends_with("README.md") {
                    (
                        "200 OK",
                        "## Sampling Parameters\n- Thinking mode for coding: temperature=0.6\n- Non-thinking mode: temperature=0.7",
                    )
                } else {
                    ("404 Not Found", "")
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                paths.push(path);
            }
            paths
        });
        let hub = HubClient::new(&format!("http://{address}"), None).unwrap();
        let dir = cached_model(3, "owner/model");
        let thinking =
            for_installed_for_mode(&hub, "owner/model", dir.path(), SamplingMode::Thinking)
                .await
                .unwrap();
        assert_eq!(thinking.values["temperature"], 0.6);
        assert_eq!(thinking.source_revision, TEST_REVISION);
        assert_eq!(
            thinking.url(&hub),
            hub.file_url("owner/model", TEST_REVISION, "README.md")
        );
        assert_eq!(
            for_installed_for_mode(&hub, "owner/model", dir.path(), SamplingMode::Thinking)
                .await
                .unwrap(),
            thinking
        );
        assert_eq!(
            for_installed_for_mode(&hub, "owner/model", dir.path(), SamplingMode::NonThinking)
                .await
                .unwrap()
                .values["temperature"],
            0.7
        );
        assert!(
            for_installed(&hub, "owner/model", dir.path())
                .await
                .is_none()
        );
        assert!(
            for_installed(&hub, "owner/model", dir.path())
                .await
                .is_none()
        );
        let paths = server.join().unwrap();
        assert_eq!(paths.len(), 4, "{paths:?}");
        assert_eq!(
            paths
                .iter()
                .filter(|path| path.ends_with("README.md"))
                .count(),
            3
        );
        assert!(
            paths
                .iter()
                .filter(|path| path.ends_with("README.md"))
                .all(|path| path.contains(TEST_REVISION))
        );
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
