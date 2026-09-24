//! The Hugging Face Hub, through its documented JSON API only.
//!
//! `GET /api/models` to search, `GET /api/models/{repo}` for one repository,
//! `GET /api/models/{repo}/tree/{revision}` for its files with sizes and
//! checksums, and `resolve/{revision}/config.json` for the model's own
//! configuration. No HTML is read. Every URL is built by path segment, never by
//! pasting text into a string, and a repository or revision that is not
//! well-formed is refused before a request is made.
//!
//! `POORAI_HF_BASE_URL` points it at another Hub (a mirror, or a test server);
//! `HF_TOKEN`, when set, is sent for gated repositories.

use crate::catalog::{self, Format, HubFile, HubModel};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// `config.json` is read into memory; anything larger is not a config.
const CONFIG_LIMIT_BYTES: usize = 2 * 1024 * 1024;

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
    fn new(kind: HubErrorKind, message: impl Into<String>) -> Self {
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
    token: Option<String>,
}

impl HubClient {
    pub fn from_env() -> Result<Self, HubError> {
        let base =
            std::env::var("POORAI_HF_BASE_URL").unwrap_or_else(|_| "https://huggingface.co".into());
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
        Ok(Self { base, http, token })
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
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
        response.json().await.map_err(|error| {
            HubError::new(
                HubErrorKind::Unexpected,
                format!("The Hub's answer could not be read: {error}"),
            )
        })
    }

    /// Repositories in `format` matching `query`, most downloaded first.
    pub async fn search(
        &self,
        query: &str,
        format: Format,
        limit: usize,
    ) -> Result<Vec<HubModel>, HubError> {
        let mut url = self.url(&["api", "models"]);
        {
            let mut pairs = url.query_pairs_mut();
            if !query.trim().is_empty() {
                pairs.append_pair("search", query.trim());
            }
            pairs
                .append_pair("filter", format.hub_filter())
                .append_pair("sort", "downloads")
                .append_pair("direction", "-1")
                .append_pair("limit", &limit.clamp(1, 50).to_string());
            for field in EXPANDED {
                pairs.append_pair("expand[]", field);
            }
        }
        let value = self.get_json(url).await?;
        Ok(value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(catalog::parse_model)
            .filter(|model| catalog::is_repository(&model.repository))
            .collect())
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
        let mut url = self.url(&["api", "models", repository, "tree", revision]);
        url.query_pairs_mut().append_pair("recursive", "true");
        let value = self.get_json(url).await?;
        Ok(catalog::parse_tree(&value))
    }

    /// The repository's `config.json` at a commit; `None` if it has none.
    pub async fn config(
        &self,
        repository: &str,
        revision: &str,
    ) -> Result<Option<serde_json::Value>, HubError> {
        check_repository(repository)?;
        check_revision(revision)?;
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
