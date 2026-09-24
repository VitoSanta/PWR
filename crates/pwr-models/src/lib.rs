//! The model manager's core: find models PWR can run, judge whether they
//! fit this machine, and download them where the engines will find them.
//!
//! - [`catalog`] maps the Hub's metadata onto runnable variants (pure);
//! - [`hub`] is the Hugging Face Hub, through its JSON API only;
//! - [`fit`] rates a variant against this machine (pure, deterministic);
//! - [`download`] fetches and verifies a plan, shared with the CLI;
//! - [`profile`] says what is known about a model (Verified, Locally
//!   calibrated, Provisional, Limited, Incompatible) and from what evidence;
//! - [`calibration`] is Quick Calibration, the bounded local check that
//!   moves a model out of Provisional.
//!
//! The front end draws what this returns and decides nothing: which variants
//! exist, how they fit, what is already on disk and what a download would
//! write are all computed here.

pub mod calibration;
pub mod catalog;
pub mod download;
pub mod fit;
pub mod hub;
pub mod local;
pub mod profile;

use catalog::{Format, HubFile, HubModel, ModelVariant};
use fit::{Capacity, FitEstimate, FitLevel, Footprint};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Repositories enriched per search: each costs a tree and a config request.
pub const SEARCH_LIMIT: usize = 20;
const CONCURRENT_REQUESTS: usize = 6;

/// A variant as a card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantEntry {
    #[serde(flatten)]
    pub variant: ModelVariant,
    pub fit: FitEstimate,
    /// On disk, from sizes alone (never re-hashed to draw a list).
    pub local: download::LocalState,
    pub local_bytes: u64,
    /// The engine already lists it, so it can be chosen now.
    pub installed: bool,
    /// Why it cannot be downloaded from here, if it cannot.
    pub blocked: Option<String>,
}

/// One repository as a card shows it. Every field is the Hub's metadata or
/// the model's own config, or `None`; nothing is inferred from a name except
/// a GGUF quantization, which says so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub repository: String,
    pub name: String,
    pub author: Option<String>,
    pub url: String,
    pub revision: Option<String>,
    pub format: Format,
    /// The engine that runs it.
    pub backend: String,
    pub base_model: Option<String>,
    pub architecture: Option<String>,
    pub architecture_source: Option<String>,
    pub parameters: Option<u64>,
    pub parameters_source: Option<String>,
    pub context_length: Option<u32>,
    pub context_source: Option<String>,
    pub license: Option<String>,
    pub downloads: Option<u64>,
    pub likes: Option<u64>,
    pub gated: bool,
    pub pipeline_tag: Option<String>,
    /// Declares a vision encoder in its config (MLX only).
    pub vision: bool,
    pub variants: Vec<VariantEntry>,
    /// The best fit among the variants, for sorting and filtering.
    pub best_fit: FitLevel,
    /// Things the person should know that are not variant-specific.
    pub notes: Vec<String>,
}

/// Filters applied to a search, here rather than in the interface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filters {
    pub compatible_only: bool,
    /// Substring of the repository or base model: "qwen", "gemma".
    pub family: Option<String>,
    pub min_parameters: Option<u64>,
    pub max_parameters: Option<u64>,
    /// Substring of a variant's quantization: "4-bit", "Q4_K".
    pub quantization: Option<String>,
    pub min_context: Option<u32>,
    pub max_bytes: Option<u64>,
}

/// Keeps what the filters allow. A variant filter (quantization, size, fit)
/// removes variants; an entry left with none is removed. An unknown value
/// never passes a bound: a model whose parameters are unknown is not "under
/// 8B".
pub fn apply_filters(entries: Vec<CatalogEntry>, filters: &Filters) -> Vec<CatalogEntry> {
    let family = filters
        .family
        .as_ref()
        .map(|f| f.trim().to_ascii_lowercase());
    let quantization = filters
        .quantization
        .as_ref()
        .map(|q| q.trim().to_ascii_lowercase());
    entries
        .into_iter()
        .filter(|entry| {
            family.as_ref().is_none_or(|family| {
                family.is_empty()
                    || entry.repository.to_ascii_lowercase().contains(family)
                    || entry
                        .base_model
                        .as_ref()
                        .is_some_and(|base| base.to_ascii_lowercase().contains(family))
            })
        })
        .filter(|entry| {
            filters
                .min_parameters
                .is_none_or(|min| entry.parameters.is_some_and(|p| p >= min))
                && filters
                    .max_parameters
                    .is_none_or(|max| entry.parameters.is_some_and(|p| p <= max))
                && filters
                    .min_context
                    .is_none_or(|min| entry.context_length.is_some_and(|c| c >= min))
        })
        .filter_map(|mut entry| {
            entry.variants.retain(|variant| {
                (!filters.compatible_only || variant.fit.level.fits() || variant.installed)
                    && filters
                        .max_bytes
                        .is_none_or(|max| variant.variant.bytes <= max)
                    && quantization.as_ref().is_none_or(|wanted| {
                        wanted.is_empty()
                            || variant
                                .variant
                                .quantization
                                .as_ref()
                                .is_some_and(|q| q.to_ascii_lowercase().contains(wanted))
                    })
            });
            (!entry.variants.is_empty()).then(|| {
                entry.best_fit = best_fit(&entry.variants);
                entry
            })
        })
        .collect()
}

/// Order of preference among fit levels, best first.
fn rank(level: FitLevel) -> u8 {
    match level {
        FitLevel::Recommended => 0,
        FitLevel::ShouldFit => 1,
        FitLevel::TightFit => 2,
        FitLevel::Unknown => 3,
        FitLevel::NotRecommended => 4,
        FitLevel::Incompatible => 5,
    }
}

fn best_fit(variants: &[VariantEntry]) -> FitLevel {
    variants
        .iter()
        .map(|variant| variant.fit.level)
        .min_by_key(|level| rank(*level))
        .unwrap_or(FitLevel::Unknown)
}

/// Where a variant's files go: `<models_root>/<owner>/<name>/<path>`, which
/// is where both engines look for a model called `<owner>/<name>`.
pub fn destination_dir(models_root: &Path, repository: &str) -> Option<PathBuf> {
    catalog::is_repository(repository).then(|| {
        let (owner, name) = repository.split_once('/').expect("checked");
        models_root.join(owner).join(name)
    })
}

/// The download plan for a variant, pinned to `revision`. Every file must
/// have a size and a checksum and a path that stays inside the destination.
pub fn plan_for(
    hub: &hub::HubClient,
    repository: &str,
    revision: &str,
    variant: &ModelVariant,
    models_root: &Path,
) -> Result<download::Plan, String> {
    if !catalog::is_revision(revision) {
        return Err(format!("{revision:?} is not a full commit id"));
    }
    let destination_root = destination_dir(models_root, repository)
        .ok_or_else(|| format!("{repository:?} is not a repository name"))?;
    let mut files = Vec::new();
    for file in &variant.files {
        if !catalog::is_safe_relative(&file.path) {
            return Err(format!("{} is not a safe file path", file.path));
        }
        if !file.verifiable() {
            return Err(format!(
                "{} has no checksum on the Hub, so it could not be verified",
                file.path
            ));
        }
        let destination = destination_root.join(&file.path);
        if !destination.starts_with(&destination_root) {
            return Err(format!(
                "{} would be written outside the model folder",
                file.path
            ));
        }
        files.push(download::PlannedFile {
            file: file.path.clone(),
            url: hub.file_url(repository, revision, &file.path),
            destination,
            expected_bytes: file.bytes,
            blake3: None,
            sha256: file.sha256.clone(),
            git_sha1: file.git_sha1.clone(),
        });
    }
    if files.is_empty() {
        return Err("the variant names no files".into());
    }
    Ok(download::Plan {
        id: format!("{repository}@{}:{}", &revision[..12], variant.id),
        destination_root,
        files,
        revision: Some(revision.to_owned()),
    })
}

/// What a card is assembled from, for [`entry`].
pub struct EntryInput<'a> {
    pub model: &'a HubModel,
    pub format: Format,
    pub files: &'a [HubFile],
    /// The repository's config.json (MLX) or its base model's (GGUF), for
    /// the memory a token of context costs.
    pub config: Option<&'a serde_json::Value>,
    pub capacity: &'a Capacity,
    pub models_root: &'a Path,
    pub installed: &'a [String],
    pub hub: &'a hub::HubClient,
    pub has_token: bool,
}

/// One card, from what the Hub said and this machine.
pub fn entry(input: EntryInput<'_>) -> CatalogEntry {
    let EntryInput {
        model,
        format,
        files,
        config,
        capacity,
        models_root,
        installed,
        hub,
        has_token,
    } = input;
    let text_config = config.map(|config| config.get("text_config").unwrap_or(config));
    let remote_code = format == Format::Mlx && config.is_some_and(catalog::needs_remote_code);
    let mut notes = Vec::new();
    if remote_code {
        notes.push(
            "Loading this model needs code shipped in its repository, which PWR does not run."
                .to_owned(),
        );
    }
    if model.is_gated() {
        notes.push(if has_token {
            "Gated: its terms must be accepted on huggingface.co for the HF_TOKEN in use."
                .to_owned()
        } else {
            "Gated: accept its terms on huggingface.co and set HF_TOKEN before starting PWR."
                .to_owned()
        });
    }
    if format == Format::Mlx && config.is_none() {
        notes.push("Its config.json could not be read, so the context cost is unknown.".to_owned());
    }
    let (architecture, architecture_source) = match (&model.gguf_architecture, text_config) {
        (Some(arch), _) => (Some(arch.clone()), Some("GGUF metadata (Hub)".to_owned())),
        (None, Some(text)) => match text.get("model_type").and_then(serde_json::Value::as_str) {
            Some(kind) => (Some(kind.to_owned()), Some("config.json".to_owned())),
            None => (None, None),
        },
        _ => (None, None),
    };
    let (parameters, parameters_source) =
        match (model.gguf_parameters, model.safetensors_parameters) {
            (Some(count), _) => (Some(count), Some("GGUF metadata (Hub)".to_owned())),
            (None, Some(count)) => (Some(count), Some("safetensors metadata (Hub)".to_owned())),
            _ => (None, None),
        };
    let config_context = text_config
        .and_then(|text| text.get("max_position_embeddings"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    let (context_length, context_source) = match (model.gguf_context_length, config_context) {
        (Some(length), _) => (Some(length), Some("GGUF metadata (Hub)".to_owned())),
        (None, Some(length)) => (Some(length), Some("config.json".to_owned())),
        _ => (None, None),
    };
    let vision = format == Format::Mlx
        && config.is_some_and(|config| {
            config
                .get("vision_config")
                .is_some_and(|value| !value.is_null())
        });
    let variants: Vec<VariantEntry> = catalog::variants(&model.repository, format, files, config)
        .into_iter()
        .map(|variant| {
            let mut fit = fit::estimate(capacity, &Footprint::new(format, variant.bytes, config));
            if remote_code {
                fit.level = FitLevel::Incompatible;
                fit.label = FitLevel::Incompatible.label().to_owned();
                fit.explanation = "This model needs custom code from its repository to load; \
                                   PWR never runs repository code."
                    .to_owned();
            }
            let plan = model.revision.as_deref().and_then(|revision| {
                plan_for(hub, &model.repository, revision, &variant, models_root).ok()
            });
            let (local, local_bytes) = plan
                .as_ref()
                .map(download::local_state)
                .unwrap_or((download::LocalState::Missing, 0));
            let blocked = if remote_code {
                Some("needs repository code".to_owned())
            } else if model.revision.is_none() {
                Some("the Hub did not report a commit to pin the download to".to_owned())
            } else if variant.files.iter().any(|file| !file.verifiable()) {
                Some("a file has no checksum on the Hub".to_owned())
            } else if model.is_gated() && !has_token {
                Some("gated: set HF_TOKEN".to_owned())
            } else {
                None
            };
            VariantEntry {
                installed: installed.contains(&variant.model_ref),
                variant,
                fit,
                local,
                local_bytes,
                blocked,
            }
        })
        .collect();
    let name = model
        .repository
        .split_once('/')
        .map_or(model.repository.as_str(), |(_, name)| name)
        .to_owned();
    CatalogEntry {
        url: hub.page_url(&model.repository),
        repository: model.repository.clone(),
        name,
        author: model.author.clone(),
        revision: model.revision.clone(),
        format,
        backend: format.backend().to_owned(),
        base_model: model.base_models.first().cloned(),
        architecture,
        architecture_source,
        parameters,
        parameters_source,
        context_length,
        context_source,
        license: model.license.clone(),
        downloads: model.downloads,
        likes: model.likes,
        gated: model.is_gated(),
        pipeline_tag: model.pipeline_tag.clone(),
        vision,
        best_fit: best_fit(&variants),
        variants,
        notes,
    }
}

/// A listing with its file tree (or why it could not be read) and config.
type Enriched = (
    HubModel,
    Result<Vec<HubFile>, hub::HubError>,
    Option<serde_json::Value>,
);

/// A search, enriched: each result's file tree and config are fetched so its
/// variants carry exact sizes and a fit. Results whose tree cannot be read are
/// kept with no variants and a note, rather than silently dropped.
pub async fn search(
    hub: &hub::HubClient,
    query: &str,
    format: Format,
    capacity: &Capacity,
    models_root: &Path,
    installed: &[String],
) -> Result<Vec<CatalogEntry>, hub::HubError> {
    use futures_util::StreamExt;
    // Asked for more than are enriched: speech, embedding and image models
    // share the formats and are dropped before any of their files are read.
    let models: Vec<HubModel> = hub
        .search(query, format, SEARCH_LIMIT * 2)
        .await?
        .into_iter()
        .filter(catalog::is_language_model)
        .take(SEARCH_LIMIT)
        .collect();
    // GGUF repositories carry no config.json; their base model's describes
    // the same architecture, so it is read once per base model.
    let bases: Vec<String> = if format == Format::Gguf {
        let mut bases: Vec<String> = models
            .iter()
            .filter_map(|model| model.base_models.first().cloned())
            .filter(|base| catalog::is_repository(base))
            .collect();
        bases.sort();
        bases.dedup();
        bases
    } else {
        Vec::new()
    };
    let base_configs: BTreeMap<String, serde_json::Value> = futures_util::stream::iter(bases)
        .map(|base| async move {
            let model = hub.model(&base).await.ok()?;
            let revision = model.revision?;
            let config = hub.config(&base, &revision).await.ok()??;
            Some((base, config))
        })
        .buffer_unordered(CONCURRENT_REQUESTS)
        .filter_map(|found| async move { found })
        .collect()
        .await;
    let enriched: Vec<Enriched> = futures_util::stream::iter(models)
        .map(|model| async move {
            let Some(revision) = model.revision.clone() else {
                return (model, Ok(Vec::new()), None);
            };
            let files = hub.tree(&model.repository, &revision).await;
            let config = if format == Format::Mlx {
                hub.config(&model.repository, &revision)
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };
            (model, files, config)
        })
        .buffered(CONCURRENT_REQUESTS)
        .collect()
        .await;
    Ok(enriched
        .into_iter()
        .map(|(model, files, config)| {
            let config = config.or_else(|| {
                model
                    .base_models
                    .first()
                    .and_then(|base| base_configs.get(base).cloned())
            });
            let (files, tree_error) = match files {
                Ok(files) => (files, None),
                Err(error) => (Vec::new(), Some(error.message)),
            };
            let mut entry = entry(EntryInput {
                model: &model,
                format,
                files: &files,
                config: config.as_ref(),
                capacity,
                models_root,
                installed,
                hub,
                has_token: hub.token().is_some(),
            });
            if let Some(error) = tree_error {
                entry
                    .notes
                    .push(format!("Its files could not be listed: {error}"));
            }
            entry
        })
        .collect())
}

/// The plan for one variant, re-read from the Hub at the pinned revision: a
/// client names what to download, never the sizes or checksums to trust.
pub async fn resolve_plan(
    hub: &hub::HubClient,
    repository: &str,
    revision: &str,
    variant_id: &str,
    format: Format,
    models_root: &Path,
) -> Result<download::Plan, String> {
    let files = hub
        .tree(repository, revision)
        .await
        .map_err(|error| error.message)?;
    let config = if format == Format::Mlx {
        let config = hub
            .config(repository, revision)
            .await
            .map_err(|error| error.message)?;
        if config.as_ref().is_some_and(catalog::needs_remote_code) {
            return Err(
                "this model needs code from its repository to load, which PWR does not run"
                    .into(),
            );
        }
        config
    } else {
        None
    };
    let variant = catalog::variants(repository, format, &files, config.as_ref())
        .into_iter()
        .find(|variant| variant.id == variant_id)
        .ok_or_else(|| format!("{repository} has no {variant_id} variant at {revision}"))?;
    let plan = plan_for(hub, repository, revision, &variant, models_root)?;
    download::require_verifiable(&plan).map_err(|error| error.message)?;
    Ok(plan)
}
