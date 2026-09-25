//! The Hugging Face Hub, through its documented JSON API only.
//!
//! `GET /api/models` to search, `GET /api/models/{repo}` for one repository,
//! `GET /api/models/{repo}/tree/{revision}` for its files with sizes and
//! checksums, and `resolve/{revision}/config.json` for the model's own
//! configuration. No HTML is read. Every URL is built by path segment, never by
//! pasting text into a string, and a repository or revision that is not
//! well-formed is refused before a request is made.
//!
//! `PWR_HF_BASE_URL` points it at another Hub (a mirror, or a test server);
//! `HF_TOKEN`, when set, is sent for gated repositories.

use crate::Filters;
use crate::catalog::{self, Format, HubFile, HubModel};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// A model file downloads for as long as data keeps arriving: only a
/// connection that goes quiet this long is given up (and then resumed).
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
/// `config.json` is read into memory; anything larger is not a config.
const CONFIG_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const CARD_LIMIT_BYTES: usize = 1024 * 1024;

fn next_cursor(link: &str) -> Option<String> {
    link.split(',').find_map(|part| {
        let (url, relation) = part.trim().split_once(';')?;
        if !relation.trim().contains("rel=\"next\"") {
            return None;
        }
        url::Url::parse(url.trim().trim_start_matches('<').trim_end_matches('>'))
            .ok()?
            .query_pairs()
            .find(|(key, _)| key == "cursor")
            .map(|(_, value)| value.into_owned())
    })
}

#[cfg(test)]
mod pagination_tests {
    use super::next_cursor;

    #[test]
    fn reads_hub_next_link_cursor() {
        let link = "<https://huggingface.co/api/models?cursor=ignored>; rel=\"prev\", <https://huggingface.co/api/models?limit=20&cursor=a%2Bb%3D>; rel=\"next\"";
        assert_eq!(next_cursor(link).as_deref(), Some("a+b="));
        assert_eq!(
            next_cursor("<https://huggingface.co/api/models>; rel=\"prev\""),
            None
        );
    }

    #[test]
    fn a_search_asks_the_hub_for_the_order_chosen() {
        let hub = super::HubClient::new("https://huggingface.co", None).unwrap();
        let sort_of = |filters: &crate::Filters| {
            let url = hub.search_url("qwen", super::Format::Mlx, filters, None, 20);
            url.query_pairs()
                .find(|(key, _)| key == "sort")
                .map(|(_, value)| value.into_owned())
        };
        assert_eq!(
            sort_of(&crate::Filters::default()).as_deref(),
            Some("downloads")
        );
        let liked = crate::Filters {
            sort: Some(crate::HubSort::Likes),
            ..Default::default()
        };
        assert_eq!(sort_of(&liked).as_deref(), Some("likes"));
        let recent: crate::Filters =
            serde_json::from_value(serde_json::json!({"sort": "lastModified"})).unwrap();
        assert_eq!(sort_of(&recent).as_deref(), Some("lastModified"));
        // Anything else is refused, not passed to the Hub.
        assert!(
            serde_json::from_value::<crate::Filters>(serde_json::json!({"sort": "size"})).is_err()
        );
    }
}

/// A repository's files and config at a commit never change, so they are
/// read once per process: the catalogue re-reads the same models whenever
/// the person changes a filter or an order, and every read counts against
/// the Hub's rate limit. Keyed by Hub, repository and full commit id.
static TREES: CacheMap<Vec<HubFile>> = std::sync::OnceLock::new();
static CONFIGS: CacheMap<Option<serde_json::Value>> = std::sync::OnceLock::new();
/// Entries kept per cache: a few thousand models' listings is a few MB.
const CACHED: usize = 4_000;

type CacheMap<T> = std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, T>>>;

fn cached<T: Clone>(cache: &CacheMap<T>, key: &str) -> Option<T> {
    cache.get()?.lock().ok()?.get(key).cloned()
}

fn remember<T>(cache: &CacheMap<T>, key: String, value: T) {
    let map = cache.get_or_init(Default::default);
    if let Ok(mut map) = map.lock() {
        if map.len() >= CACHED {
            map.clear();
        }
        map.insert(key, value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubErrorKind {
    /// No connection could be made: offline, DNS, a firewall.
    Offline,
    RateLimited,
    NotFound,
    /// The repository needs its terms accepted and a token.
    Gated,
    /// The Hub answered with something this client cannot read, or an error.
    Unexpected,
    /// The request was refused before it was made.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubError {
    pub kind: HubErrorKind,
    pub message: String,
}

impl HubError {
    pub(crate) fn new(kind: HubErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Clone)]
pub struct HubClient {
    base: url::Url,
    http: reqwest::Client,
    transfer: reqwest::Client,
    token: Option<String>,
}

pub struct ModelPage {
    pub models: Vec<HubModel>,
    pub next_cursor: Option<String>,
}

impl HubClient {
    pub fn from_env() -> Result<Self, HubError> {
        let base =
            std::env::var("PWR_HF_BASE_URL").unwrap_or_else(|_| "https://huggingface.co".into());
        let token = std::env::var("HF_TOKEN")
            .ok()
            .filter(|token| !token.trim().is_empty());
        Self::new(&base, token)
    }

    pub fn new(base: &str, token: Option<String>) -> Result<Self, HubError> {
        let base = url::Url::parse(base).map_err(|error| {
            HubError::new(
                HubErrorKind::Invalid,
                format!("{base} is not a URL: {error}"),
            )
        })?;
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("PWR/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| HubError::new(HubErrorKind::Unexpected, error.to_string()))?;
        // Not `http`: its timeout covers the whole response, body included,
        // and cut every model file that took longer than that to arrive.
        let transfer = reqwest::Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .read_timeout(STALL_TIMEOUT)
            .user_agent(concat!("PWR/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| HubError::new(HubErrorKind::Unexpected, error.to_string()))?;
        Ok(Self {
            base,
            http,
            transfer,
            token,
        })
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// For API requests: bounded end to end.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// For model files: no limit on the whole transfer, only on a stall.
    pub fn transfer(&self) -> &reqwest::Client {
        &self.transfer
    }

    fn url(&self, segments: &[&str]) -> url::Url {
        let mut url = self.base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .expect("an http(s) base URL has a path");
            path.pop_if_empty();
            for segment in segments {
                // A repository is `owner/name`: two segments, each escaped.
                for part in segment.split('/') {
                    path.push(part);
                }
            }
        }
        url
    }

    /// The URL a file of a repository is downloaded from, pinned to a commit.
    pub fn file_url(&self, repository: &str, revision: &str, path: &str) -> String {
        let mut segments = vec![repository, "resolve", revision];
        segments.push(path);
        self.url(&segments).to_string()
    }

    /// The page a person opens to read about the model.
    pub fn page_url(&self, repository: &str) -> String {
        self.url(&[repository]).to_string()
    }

    async fn get_json(&self, url: url::Url) -> Result<serde_json::Value, HubError> {
        self.get_json_page(url).await.map(|(value, _)| value)
    }

    async fn get_json_page(
        &self,
        url: url::Url,
    ) -> Result<(serde_json::Value, Option<String>), HubError> {
        let mut request = self.http.get(url.clone());
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_connect() || error.is_timeout() {
                HubError::new(
                    HubErrorKind::Offline,
                    format!(
                        "Hugging Face could not be reached ({}). Check the network connection and retry.",
                        self.base.host_str().unwrap_or("the Hub")
                    ),
                )
            } else {
                HubError::new(HubErrorKind::Offline, format!("the request to {url} failed: {error}"))
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => HubError::new(
                    HubErrorKind::Gated,
                    "This repository is gated: accept its terms on huggingface.co, then set HF_TOKEN \
                     before starting PWR.",
                ),
                404 => HubError::new(
                    HubErrorKind::NotFound,
                    "The Hub has no such repository or revision.",
                ),
                429 => HubError::new(
                    HubErrorKind::RateLimited,
                    "The Hub is rate-limiting requests from this address. Wait a minute and retry.",
                ),
                _ => HubError::new(
                    HubErrorKind::Unexpected,
                    format!("The Hub answered HTTP {status}."),
                ),
            });
        }
        let next_cursor = response
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|header| header.to_str().ok())
            .and_then(next_cursor);
        let value = response.json().await.map_err(|error| {
            HubError::new(
                HubErrorKind::Unexpected,
                format!("The Hub's answer could not be read: {error}"),
            )
        })?;
        Ok((value, next_cursor))
    }

    /// Repositories in `format` matching `query`, in the order `filters`
    /// asks for (most downloaded first by default).
    pub async fn search_page(
        &self,
        query: &str,
        format: Format,
        filters: &Filters,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ModelPage, HubError> {
        let url = self.search_url(query, format, filters, cursor, limit);
        let (value, next_cursor) = self.get_json_page(url).await?;
        let models = value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(catalog::parse_model)
            .filter(|model| catalog::is_repository(&model.repository))
            .collect();
        Ok(ModelPage {
            models,
            next_cursor,
        })
    }

    fn search_url(
        &self,
        query: &str,
        format: Format,
        filters: &Filters,
        cursor: Option<&str>,
        limit: usize,
    ) -> url::Url {
        let mut url = self.url(&["api", "models"]);
        {
            let mut pairs = url.query_pairs_mut();
            if !query.trim().is_empty() {
                pairs.append_pair("search", query.trim());
            }
            pairs
                .append_pair("filter", format.hub_filter())
                .append_pair(
                    "sort",
                    filters.sort.unwrap_or(crate::HubSort::Downloads).as_str(),
                )
                .append_pair("direction", "-1")
                .append_pair("limit", &limit.clamp(1, 50).to_string());
            let parameter_range = [
                filters.min_parameters.map(|min| format!("min:{min}")),
                filters.max_parameters.map(|max| format!("max:{max}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(",");
            if !parameter_range.is_empty() {
                pairs.append_pair("num_parameters", &parameter_range);
            }
            for field in EXPANDED {
                pairs.append_pair("expand[]", field);
            }
            if let Some(cursor) = cursor {
                pairs.append_pair("cursor", cursor);
            }
        }
        url
    }

    /// One repository's listing, at its current commit.
    pub async fn model(&self, repository: &str) -> Result<HubModel, HubError> {
        check_repository(repository)?;
        let mut url = self.url(&["api", "models", repository]);
        {
            let mut pairs = url.query_pairs_mut();
            for field in EXPANDED {
                pairs.append_pair("expand[]", field);
            }
        }
        let value = self.get_json(url).await?;
        catalog::parse_model(&value).ok_or_else(|| {
            HubError::new(HubErrorKind::Unexpected, "The Hub's answer named no model.")
        })
    }

    /// Every file of a repository at a commit, with sizes and checksums.
    pub async fn tree(&self, repository: &str, revision: &str) -> Result<Vec<HubFile>, HubError> {
        check_repository(repository)?;
        check_revision(revision)?;
        let key = self.cache_key(repository, revision);
        if let Some(files) = cached(&TREES, &key) {
            return Ok(files);
        }
        let mut url = self.url(&["api", "models", repository, "tree", revision]);
        url.query_pairs_mut().append_pair("recursive", "true");
        let value = self.get_json(url).await?;
        let files = catalog::parse_tree(&value);
        remember(&TREES, key, files.clone());
        Ok(files)
    }

    /// A repository at a commit, for the caches below.
    fn cache_key(&self, repository: &str, revision: &str) -> String {
        format!("{}|{repository}|{revision}", self.base)
    }

    /// The repository's `config.json` at a commit; `None` if it has none.
    pub async fn config(
        &self,
        repository: &str,
        revision: &str,
    ) -> Result<Option<serde_json::Value>, HubError> {
        check_repository(repository)?;
        check_revision(revision)?;
        let key = self.cache_key(repository, revision);
        if let Some(config) = cached(&CONFIGS, &key) {
            return Ok(config);
        }
        let config = self.fetch_config(repository, revision).await?;
        remember(&CONFIGS, key, config.clone());
        Ok(config)
    }

    async fn fetch_config(
        &self,
        repository: &str,
        revision: &str,
    ) -> Result<Option<serde_json::Value>, HubError> {
        let url = self.url(&[repository, "resolve", revision, "config.json"]);
        let mut request = self.http.get(url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            HubError::new(
                HubErrorKind::Offline,
                format!("config.json could not be fetched: {error}"),
            )
        })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        // Not "no config": read as one, a rate-limited answer rated the model
        // without its context cost, and the rating was kept.
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(HubError::new(
                HubErrorKind::RateLimited,
                "The Hub is rate-limiting requests from this address. Wait a minute and retry.",
            ));
        }
        if !response.status().is_success() {
            return Ok(None);
        }
        if response
            .content_length()
            .is_some_and(|length| length as usize > CONFIG_LIMIT_BYTES)
        {
            return Ok(None);
        }
        let bytes = response.bytes().await.map_err(|error| {
            HubError::new(
                HubErrorKind::Offline,
                format!("config.json could not be read: {error}"),
            )
        })?;
        if bytes.len() > CONFIG_LIMIT_BYTES {
            return Ok(None);
        }
        Ok(serde_json::from_slice(&bytes).ok())
    }

    /// A model card at an exact commit. Only its bounded text is read; model
    /// repositories cannot supply code or instructions to the agent through
    /// this path.
    pub async fn card(&self, repository: &str, revision: &str) -> Result<Option<String>, HubError> {
        check_repository(repository)?;
        check_revision(revision)?;
        let url = self.url(&[repository, "resolve", revision, "README.md"]);
        let mut request = self.http.get(url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            HubError::new(
                HubErrorKind::Offline,
                format!("model card could not be fetched: {error}"),
            )
        })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(HubError::new(
                HubErrorKind::Unexpected,
                format!("model card request returned HTTP {}", response.status()),
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > CARD_LIMIT_BYTES as u64)
        {
            return Ok(None);
        }
        let bytes = response.bytes().await.map_err(|error| {
            HubError::new(
                HubErrorKind::Offline,
                format!("model card could not be read: {error}"),
            )
        })?;
        if bytes.len() > CARD_LIMIT_BYTES {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes.to_vec()).ok())
    }
}

/// Listing fields asked for: what a card shows, from the Hub's own metadata.
const EXPANDED: [&str; 12] = [
    "author",
    "cardData",
    "downloads",
    "likes",
    "gated",
    "gguf",
    "safetensors",
    "sha",
    "tags",
    "pipeline_tag",
    "library_name",
    "lastModified",
];

fn check_repository(repository: &str) -> Result<(), HubError> {
    if catalog::is_repository(repository) {
        Ok(())
    } else {
        Err(HubError::new(
            HubErrorKind::Invalid,
            format!("{repository:?} is not a repository name (owner/name)"),
        ))
    }
}

fn check_revision(revision: &str) -> Result<(), HubError> {
    if catalog::is_revision(revision) {
        Ok(())
    } else {
        Err(HubError::new(
            HubErrorKind::Invalid,
            format!("{revision:?} is not a full commit id"),
        ))
    }
}
