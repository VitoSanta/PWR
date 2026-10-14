//! PWR's models on disk: the Gemma 4 packs, downloaded from the registry
//! (`strategies/artifacts.json`) where the engines will find them.
//!
//! - [`catalog`] names the formats PWR has an engine for;
//! - [`download`] fetches and verifies a pinned plan, file by file;
//! - [`local`] lists and removes what is in the models folders.
//!
//! Which packs exist, their tiers and their memory are in
//! `pwr_domain::gemma`; nothing here searches for or rates other models.

pub mod catalog;
pub mod download;
pub mod local;
