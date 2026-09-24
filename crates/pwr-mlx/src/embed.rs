//! The embedding sidecar's client: texts in, unit vectors out.
//!
//! Blocking and line-oriented, because its one caller -- retrieval, while a
//! turn is being composed -- is synchronous and waits for the answer anyway.
//! A separate process from the engine's sidecar so that embedding a query
//! never queues behind a generation nor touches the engine's prompt cache.
//!
//! Environment:
//! - `POORAI_EMBED_PYTHON`: the interpreter with `mlx-embeddings` (default:
//!   `POORAI_MLX_PYTHON`, then `python3`);
//! - `POORAI_EMBED_SIDECAR`: the script (default: the one in this crate);
//! - `POORAI_EMBED_MODEL`, `POORAI_EMBED_POOLING`: passed through to it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Which side of a comparison a text is on. Some encoders (e5) were trained
/// with a different prefix for each, and rank worse without them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedKind {
    Query,
    Passage,
}

impl EmbedKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Passage => "passage",
        }
    }
}

pub struct Embedder {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// The model the sidecar loaded, as it reported it: part of the cache key,
    /// because vectors from two models are not comparable.
    pub model: String,
}

impl Embedder {
    /// Starts the sidecar and waits for it to load its model.
    ///
    /// An error here means "no semantic ranking this time", never a failed
    /// turn: the caller falls back to the lexical ranking.
    pub fn start() -> Result<Self, String> {
        let python = std::env::var_os("POORAI_EMBED_PYTHON")
            .or_else(|| std::env::var_os("POORAI_MLX_PYTHON"))
            .map_or_else(|| PathBuf::from("python3"), PathBuf::from);
        let script = std::env::var_os("POORAI_EMBED_SIDECAR").map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("sidecar/pwr_embed.py"),
            PathBuf::from,
        );
        let mut child = Command::new(&python)
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "could not start the embedding sidecar with {}: {error}",
                    python.display()
                )
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or("the embedding sidecar has no input")?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("the embedding sidecar has no output")?,
        );
        let mut embedder = Self {
            child,
            stdin,
            stdout,
            model: String::new(),
        };
        let ready = embedder.read_line()?;
        if let Some(error) = ready.get("error").and_then(serde_json::Value::as_str) {
            return Err(error.to_owned());
        }
        embedder.model = ready
            .get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        Ok(embedder)
    }

    /// One unit vector per text, in order.
    pub fn embed(&mut self, kind: EmbedKind, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let request = serde_json::json!({"kind": kind.as_str(), "texts": texts});
        writeln!(self.stdin, "{request}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| format!("the embedding sidecar stopped reading: {error}"))?;
        let reply = self.read_line()?;
        if let Some(error) = reply.get("error").and_then(serde_json::Value::as_str) {
            return Err(error.to_owned());
        }
        let vectors: Vec<Vec<f32>> = reply
            .get("vectors")
            .and_then(serde_json::Value::as_array)
            .ok_or("the embedding sidecar answered without vectors")?
            .iter()
            .map(|vector| {
                vector
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(serde_json::Value::as_f64)
                            .map(|value| value as f32)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        if vectors.len() != texts.len() {
            return Err(format!(
                "the embedding sidecar returned {} vectors for {} texts",
                vectors.len(),
                texts.len()
            ));
        }
        Ok(vectors)
    }

    fn read_line(&mut self) -> Result<serde_json::Value, String> {
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .map_err(|error| format!("the embedding sidecar stopped answering: {error}"))?;
        if read == 0 {
            return Err(
                "the embedding sidecar exited; is mlx-embeddings installed and the model cached?"
                    .into(),
            );
        }
        serde_json::from_str(&line).map_err(|error| {
            format!("the embedding sidecar answered something unreadable: {error}")
        })
    }
}

impl Drop for Embedder {
    fn drop(&mut self) {
        // Closing its input ends its loop; killing covers a sidecar stuck in a
        // load. Neither may outlive the command that started it.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Cosine similarity of two unit vectors: their dot product.
pub fn similarity(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}
