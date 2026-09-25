//! Hub metadata mapped onto what PWR can run.
//!
//! Pure functions over the JSON the Hugging Face Hub API returns: a model's
//! listing, its file tree at one revision, and its `config.json`. Nothing here
//! does I/O, so every mapping is tested against recorded responses.
//!
//! Only two formats are offered, because only two run here: MLX (PWR's own
//! engine, Apple Silicon) and GGUF (llama.cpp). A repository that needs custom
//! code to load (`auto_map` in its config) is marked incompatible rather than
//! offered: PWR never runs code from a model repository.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A format PWR has an engine for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Mlx,
    Gguf,
}

impl Format {
    /// The engine that runs it, as `--backend` names it.
    pub fn backend(self) -> &'static str {
        match self {
            Format::Mlx => "mlx",
            Format::Gguf => "llama",
        }
    }

    /// The Hub's library filter for it.
    pub fn hub_filter(self) -> &'static str {
        match self {
            Format::Mlx => "mlx",
            Format::Gguf => "gguf",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mlx" => Some(Format::Mlx),
            "gguf" | "llama" | "llama.cpp" => Some(Format::Gguf),
            _ => None,
        }
    }
}

/// One file of a repository at a pinned revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubFile {
    pub path: String,
    pub bytes: u64,
    /// The LFS object id, which is the file's SHA-256.
    pub lfs_sha256: Option<String>,
    /// The git blob id (SHA-1 of `blob <len>\0<content>`), for files stored in
    /// git itself rather than LFS.
    pub git_oid: Option<String>,
}

/// What the Hub lists about a repository. Every field is what the API said,
/// or `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubModel {
    pub repository: String,
    pub author: Option<String>,
    /// The commit the listing describes. Downloads are pinned to it.
    pub revision: Option<String>,
    pub downloads: Option<u64>,
    pub likes: Option<u64>,
    pub license: Option<String>,
    pub base_models: Vec<String>,
    pub pipeline_tag: Option<String>,
    pub tags: Vec<String>,
    /// `false`, or the gating mode ("auto", "manual") the Hub reports.
    pub gated: Option<String>,
    pub last_modified: Option<String>,
    /// From the Hub's GGUF summary: parameters, architecture, trained length.
    pub gguf_parameters: Option<u64>,
    pub gguf_architecture: Option<String>,
    pub gguf_context_length: Option<u32>,
    /// From the Hub's safetensors summary.
    pub safetensors_parameters: Option<u64>,
}

impl HubModel {
    /// The parameter count the Hub lists, when it is believable. The Hub
    /// counts the tensors in the safetensors files, and for packed quantized
    /// weights that undercounts: a 19B model at 2 bits was listed at a few
    /// million. When the name states a size and the count is under a
    /// quarter of it, the count is not believed.
    pub fn listed_parameters(&self) -> Option<u64> {
        let listed = self.gguf_parameters.or(self.safetensors_parameters)?;
        match named_parameters(&self.repository) {
            Some(named) if listed < named / 4 => None,
            _ => Some(listed),
        }
    }

    pub fn is_gated(&self) -> bool {
        self.gated.is_some()
    }

    pub fn format(&self) -> Option<Format> {
        if self.tags.iter().any(|tag| tag == "gguf") {
            Some(Format::Gguf)
        } else if self.tags.iter().any(|tag| tag == "mlx") {
            Some(Format::Mlx)
        } else {
            None
        }
    }
}

/// Pipelines a conversation can use: text in, text out, possibly with images.
/// A repository that names none is kept (many quantizers omit it); one that
/// names speech recognition, embeddings or image generation is not a model
/// PWR can talk to, whatever its format.
const CONVERSATIONAL_PIPELINES: [&str; 5] = [
    "text-generation",
    "image-text-to-text",
    "any-to-any",
    "conversational",
    "text2text-generation",
];

pub fn is_language_model(model: &HubModel) -> bool {
    model
        .pipeline_tag
        .as_deref()
        .is_none_or(|tag| CONVERSATIONAL_PIPELINES.contains(&tag))
}

/// Reads one entry of `/api/models` or `/api/models/{repo}`.
pub fn parse_model(value: &Value) -> Option<HubModel> {
    let repository = value
        .get("id")
        .or_else(|| value.get("modelId"))
        .and_then(Value::as_str)?
        .to_owned();
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let card = value.get("cardData");
    let license = card
        .and_then(|card| card.get("license"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            string_list(value.get("tags"))
                .into_iter()
                .find_map(|tag| tag.strip_prefix("license:").map(str::to_owned))
        });
    let base_models = match card.and_then(|card| card.get("base_model")) {
        Some(Value::String(one)) => vec![one.clone()],
        Some(list @ Value::Array(_)) => string_list(Some(list)),
        _ => Vec::new(),
    };
    let gated = match value.get("gated") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(Value::Bool(true)) => Some("true".to_owned()),
        Some(Value::String(mode)) => Some(mode.clone()),
        Some(other) => Some(other.to_string()),
    };
    let gguf = value.get("gguf");
    Some(HubModel {
        author: text("author").or_else(|| repository.split_once('/').map(|(a, _)| a.to_owned())),
        revision: text("sha").filter(|sha| is_revision(sha)),
        downloads: value.get("downloads").and_then(Value::as_u64),
        likes: value.get("likes").and_then(Value::as_u64),
        license,
        base_models,
        pipeline_tag: text("pipeline_tag"),
        tags: string_list(value.get("tags")),
        gated,
        last_modified: text("lastModified"),
        gguf_parameters: gguf.and_then(|g| g.get("total")).and_then(Value::as_u64),
        gguf_architecture: gguf
            .and_then(|g| g.get("architecture"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        gguf_context_length: gguf
            .and_then(|g| g.get("context_length"))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        safetensors_parameters: value
            .get("safetensors")
            .and_then(|s| s.get("total"))
            .and_then(Value::as_u64),
        repository,
    })
}

/// Reads `/api/models/{repo}/tree/{revision}?recursive=true`.
pub fn parse_tree(value: &Value) -> Vec<HubFile> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?.to_owned();
            let lfs = entry.get("lfs");
            let bytes = lfs
                .and_then(|lfs| lfs.get("size"))
                .or_else(|| entry.get("size"))
                .and_then(Value::as_u64)?;
            Some(HubFile {
                path,
                bytes,
                lfs_sha256: lfs
                    .and_then(|lfs| lfs.get("oid"))
                    .and_then(Value::as_str)
                    .filter(|oid| is_hex(oid, 64))
                    .map(str::to_owned),
                git_oid: entry
                    .get("oid")
                    .and_then(Value::as_str)
                    .filter(|oid| is_hex(oid, 40))
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// A file of a variant, with the checksum it will be verified against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub git_sha1: Option<String>,
}

impl VariantFile {
    pub fn verifiable(&self) -> bool {
        self.sha256.is_some() || self.git_sha1.is_some()
    }
}

/// One downloadable, runnable artifact inside a repository: the whole MLX
/// folder, or one GGUF quantization (all its shards).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelVariant {
    /// Stable within the repository: the GGUF file (first shard) or `mlx`.
    pub id: String,
    pub format: Format,
    pub quantization: Option<String>,
    /// Where the quantization label came from: `config` (the model's own
    /// config.json), `tag` (the repository's tags) or `filename`.
    pub quantization_source: Option<String>,
    pub files: Vec<VariantFile>,
    pub bytes: u64,
    /// The local model reference the engine will know it by once downloaded.
    pub model_ref: String,
}

/// Files an MLX model folder needs. Anything else -- documentation, images,
/// and above all Python -- is left behind.
fn mlx_file_wanted(path: &str) -> bool {
    if path.contains('/') {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    [
        ".json",
        ".safetensors",
        ".txt",
        ".model",
        ".tiktoken",
        ".jinja",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

/// The variants a repository offers in `format`, with the file sizes and
/// checksums of `files`. `config` is the repository's `config.json`, read for
/// an MLX quantization; `None` when it could not be read.
pub fn variants(
    repository: &str,
    format: Format,
    files: &[HubFile],
    config: Option<&Value>,
) -> Vec<ModelVariant> {
    match format {
        Format::Mlx => mlx_variant(repository, files, config).into_iter().collect(),
        Format::Gguf => gguf_variants(repository, files),
    }
}

fn to_variant_file(file: &HubFile) -> VariantFile {
    VariantFile {
        path: file.path.clone(),
        bytes: file.bytes,
        // An LFS file's git blob is the pointer, not the content, so only
        // its SHA-256 describes what is downloaded.
        sha256: file.lfs_sha256.clone(),
        git_sha1: if file.lfs_sha256.is_none() {
            file.git_oid.clone()
        } else {
            None
        },
    }
}

fn mlx_variant(
    repository: &str,
    files: &[HubFile],
    config: Option<&Value>,
) -> Option<ModelVariant> {
    let has_config = files.iter().any(|file| file.path == "config.json");
    let has_weights = files
        .iter()
        .any(|file| !file.path.contains('/') && file.path.ends_with(".safetensors"));
    if !has_config || !has_weights {
        return None;
    }
    let chosen: Vec<VariantFile> = files
        .iter()
        .filter(|file| mlx_file_wanted(&file.path))
        .map(to_variant_file)
        .collect();
    let (quantization, source) = match config.and_then(mlx_quantization) {
        Some(bits) => (Some(bits), Some("config")),
        None => (None, None),
    };
    Some(ModelVariant {
        id: "mlx".into(),
        format: Format::Mlx,
        quantization,
        quantization_source: source.map(str::to_owned),
        bytes: chosen.iter().map(|file| file.bytes).sum(),
        files: chosen,
        model_ref: repository.to_owned(),
    })
}

/// `quantization.bits` from an MLX config, as "4-bit"; `None` for an
/// unquantized model, which is said by the dtype rather than guessed.
pub fn mlx_quantization(config: &Value) -> Option<String> {
    let quantization = config
        .get("quantization")
        .or_else(|| config.get("quantization_config"))?;
    let bits = quantization.get("bits").and_then(Value::as_u64)?;
    Some(match quantization.get("mode").and_then(Value::as_str) {
        Some(mode) if mode != "affine" => format!("{bits}-bit {mode}"),
        _ => format!("{bits}-bit"),
    })
}

/// Whether loading the model needs code shipped in its repository.
pub fn needs_remote_code(config: &Value) -> bool {
    config.get("auto_map").is_some()
        || config
            .get("text_config")
            .and_then(|text| text.get("auto_map"))
            .is_some()
}

fn gguf_variants(repository: &str, files: &[HubFile]) -> Vec<ModelVariant> {
    let mut groups: Vec<(String, Vec<&HubFile>)> = Vec::new();
    for file in files {
        let lower = file.path.to_ascii_lowercase();
        if !lower.ends_with(".gguf") {
            continue;
        }
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        // A vision projector or an importance matrix is not a model.
        if name.starts_with("mmproj") || name.contains("imatrix") {
            continue;
        }
        let key = shard_group(&file.path);
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, members)) => members.push(file),
            None => groups.push((key, vec![file])),
        }
    }
    groups
        .into_iter()
        .filter_map(|(key, mut members)| {
            members.sort_by(|a, b| a.path.cmp(&b.path));
            // A split model is only runnable whole.
            if let Some(expected) = shard_count(&members[0].path)
                && members.len() != expected
            {
                return None;
            }
            let files: Vec<VariantFile> =
                members.iter().map(|file| to_variant_file(file)).collect();
            let first = files[0].path.clone();
            let quantization = gguf_quantization(&key);
            Some(ModelVariant {
                id: first.clone(),
                format: Format::Gguf,
                quantization_source: quantization.as_ref().map(|_| "filename".to_owned()),
                quantization,
                bytes: files.iter().map(|file| file.bytes).sum(),
                files,
                model_ref: format!("{repository}/{first}"),
            })
        })
        .collect()
}

/// The shard suffix `-00001-of-00003` removed, so a split file's parts group.
fn shard_group(path: &str) -> String {
    let stem = path.strip_suffix(".gguf").unwrap_or(path);
    match stem.rsplit_once("-of-") {
        Some((head, total)) if total.len() == 5 && total.bytes().all(|b| b.is_ascii_digit()) => {
            match head.rsplit_once('-') {
                Some((base, part))
                    if part.len() == 5 && part.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    base.to_owned()
                }
                _ => stem.to_owned(),
            }
        }
        _ => stem.to_owned(),
    }
}

fn shard_count(path: &str) -> Option<usize> {
    let stem = path.strip_suffix(".gguf").unwrap_or(path);
    let (_, total) = stem.rsplit_once("-of-")?;
    (total.len() == 5).then(|| total.parse().ok()).flatten()
}

/// The quantization named in a GGUF file name, such as `Q4_K_M`, `IQ3_XXS`,
/// `UD-Q4_K_XL`, `BF16` or `MXFP4`. Read from the name because the name is all
/// the tree says; the label says so.
pub fn gguf_quantization(name: &str) -> Option<String> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let upper = base.to_ascii_uppercase();
    let tokens: Vec<&str> = upper.split(['-', '.', '/']).collect();
    for (index, token) in tokens.iter().enumerate().rev() {
        let quant = is_quant_token(token);
        if quant {
            // Unsloth's dynamic quants are named `UD-Q4_K_XL`.
            if index > 0 && tokens[index - 1] == "UD" {
                return Some(format!("UD-{token}"));
            }
            return Some((*token).to_owned());
        }
    }
    None
}

fn is_quant_token(token: &str) -> bool {
    if matches!(token, "F16" | "F32" | "BF16" | "FP16" | "MXFP4" | "FP8") {
        return true;
    }
    let rest = token
        .strip_prefix("IQ")
        .or_else(|| token.strip_prefix('Q'))
        .or_else(|| token.strip_prefix("TQ"));
    match rest {
        Some(rest) => {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            digits > 0
                && rest[digits..]
                    .chars()
                    .all(|c| c == '_' || c.is_ascii_alphanumeric())
        }
        None => false,
    }
}

/// A repository name as the Hub requires it: `owner/name`, each part letters,
/// digits, `-`, `_` and `.`, and neither part `.` or `..`.
pub fn is_repository(repository: &str) -> bool {
    let Some((owner, name)) = repository.split_once('/') else {
        return false;
    };
    let part = |part: &str| {
        !part.is_empty()
            && part.len() <= 96
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    part(owner) && part(name)
}

/// A full commit id: 40 hex characters, never a branch or tag.
pub fn is_revision(revision: &str) -> bool {
    is_hex(revision, 40)
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A relative path inside a repository that cannot escape the folder it is
/// written to.
pub fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The size a repository's name states, in parameters: `Qwen3-8B`, `0.5B`,
/// `8x22b` (experts times size). An active (`A3B`) or effective (`E4B`) size
/// is not the model's size and is not read. The largest stated is taken.
pub fn named_parameters(repository: &str) -> Option<u64> {
    let name = repository.rsplit('/').next()?.to_ascii_lowercase();
    let billions = |text: &str| -> Option<f64> {
        let number = text.strip_suffix('b')?;
        if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        number.parse::<f64>().ok()
    };
    name.split(['-', '_'])
        .filter_map(|token| match token.split_once('x') {
            Some((experts, size)) => Some(experts.parse::<f64>().ok()? * billions(size)?),
            None => billions(token),
        })
        .filter(|size| *size > 0.0)
        .max_by(f64::total_cmp)
        .map(|size| (size * 1e9) as u64)
}

#[cfg(test)]
mod named_tests {
    use super::*;

    #[test]
    fn a_name_states_the_size_and_contradicts_a_packed_count() {
        const B: u64 = 1_000_000_000;
        assert_eq!(
            named_parameters("neopolita/Qwen3.6-19B-A3B-Niwaki-v2-2bit-mlx"),
            Some(19 * B)
        );
        assert_eq!(
            named_parameters("mlx-community/SorcererLM-8x22b-2bit"),
            Some(176 * B)
        );
        assert_eq!(named_parameters("Qwen/Qwen2.5-0.5B-Instruct"), Some(B / 2));
        assert_eq!(
            named_parameters("lmstudio-community/gemma-4-E4B-it-MLX-4bit"),
            None
        );
        assert_eq!(named_parameters("rishabhguptajs/tinystories-10m-mlx"), None);
        let listed = |name: &str, count| HubModel {
            repository: name.into(),
            safetensors_parameters: Some(count),
            ..Default::default()
        };
        // Packed 2-bit weights counted as a few million: not believed.
        assert_eq!(
            listed("n/Qwen3.6-19B-A3B-2bit-mlx", 30_000_000).listed_parameters(),
            None
        );
        // A believable count is the Hub's, even where it differs a little.
        assert_eq!(
            listed("n/Qwen3-8B-4bit", 8_190_000_000).listed_parameters(),
            Some(8_190_000_000)
        );
        // No size in the name: nothing to contradict it.
        assert_eq!(
            listed("r/tinystories-10m-mlx", 10_000_000).listed_parameters(),
            Some(10_000_000)
        );
    }
}
