//! What a model file is: the formats PWR has an engine for, and the shape
//! of a pinned revision.

use serde::{Deserialize, Serialize};

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

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mlx" => Some(Format::Mlx),
            "gguf" | "llama" | "llama.cpp" => Some(Format::Gguf),
            _ => None,
        }
    }
}

/// A full commit id: 40 hex characters, never a branch or tag.
pub fn is_revision(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_revisions_are_read_strictly() {
        assert_eq!(Format::parse("MLX"), Some(Format::Mlx));
        assert_eq!(Format::parse("llama.cpp"), Some(Format::Gguf));
        assert_eq!(Format::parse("onnx"), None);
        assert!(is_revision("0d77464eeb233a2da68ebf9d7dc4edaac7db956d"));
        assert!(!is_revision("main"));
    }
}
