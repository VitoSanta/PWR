//! Semantic ranking of document sections, from a local embedding model.
//!
//! The lexical ranking finds a section when it shares words with the request;
//! this one finds it when it shares meaning, which is the case the lexical one
//! cannot reach ("why does this project exist" -> "Definition and initial
//! user"). Fused with it by reciprocal rank in `pwr_repo::retrieve_with`,
//! measured 2026-09-23 as the best ranking of every arm tried (backlog C.22).
//!
//! Local and offline: the encoder runs in PWR's embedding sidecar and never
//! downloads anything. Section vectors are cached in the workspace by content
//! hash and model, so a document is embedded once and re-embedded only when
//! it changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pwr_mlx::embed::{EmbedKind, Embedder, similarity};
use pwr_repo::{SectionRanker, SectionText};

pub(crate) struct EmbeddingRanker {
    embedder: Embedder,
    cache: HashMap<String, Vec<f32>>,
    path: PathBuf,
    dirty: bool,
    /// Sections embedded by this ranker, as opposed to read from the cache:
    /// what a caller reports when it says what semantic ranking cost.
    pub(crate) embedded: usize,
}

impl EmbeddingRanker {
    /// Starts the encoder and reads this workspace's cache for its model.
    /// An error means "rank lexically this time", never a failed turn.
    pub(crate) fn open(root: &Path) -> Result<Self, String> {
        let embedder = Embedder::start()?;
        let slug: String = embedder
            .model
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = root.join(".poorai/embeddings").join(format!("{slug}.json"));
        let cache = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<HashMap<String, Vec<f32>>>(&text).ok())
            .unwrap_or_default();
        Ok(Self {
            embedder,
            cache,
            path,
            dirty: false,
            embedded: 0,
        })
    }

    pub(crate) fn model(&self) -> &str {
        &self.embedder.model
    }

    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string(&self.cache)
            && std::fs::write(&self.path, text).is_ok()
        {
            self.dirty = false;
        }
    }
}

impl SectionRanker for EmbeddingRanker {
    fn similarities(&mut self, query: &str, sections: &[SectionText<'_>]) -> Option<Vec<f32>> {
        let keys: Vec<String> = sections
            .iter()
            .map(|section| pwr_domain::hash_bytes(section.text))
            .collect();
        let mut missing: Vec<(String, String)> = Vec::new();
        for (key, section) in keys.iter().zip(sections) {
            if !self.cache.contains_key(key) && !missing.iter().any(|(seen, _)| seen == key) {
                missing.push((key.clone(), section.text.to_owned()));
            }
        }
        if !missing.is_empty() {
            let texts: Vec<String> = missing.iter().map(|(_, text)| text.clone()).collect();
            let vectors = self.embedder.embed(EmbedKind::Passage, &texts).ok()?;
            for ((key, _), vector) in missing.into_iter().zip(vectors) {
                self.cache.insert(key, vector);
            }
            self.embedded += texts.len();
            self.dirty = true;
            self.save();
        }
        let query = self
            .embedder
            .embed(EmbedKind::Query, &[query.to_owned()])
            .ok()?
            .pop()?;
        Some(
            keys.iter()
                .map(|key| {
                    self.cache
                        .get(key)
                        .map_or(0.0, |vector| similarity(&query, vector))
                })
                .collect(),
        )
    }
}

impl Drop for EmbeddingRanker {
    fn drop(&mut self) {
        self.save();
    }
}
