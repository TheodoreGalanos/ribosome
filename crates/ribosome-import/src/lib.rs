//! Optional ingestion of external trajectories into Ribosome's existing store.
pub mod decode;
pub mod profile;
pub mod reader;
#[cfg(feature = "remote")]
mod remote;
pub mod study;

pub use ribosome_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bounds apply before an episode becomes visible in the evidence store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub episodes: usize,
    pub row_bytes: usize,
    pub messages: usize,
    pub table_rows: usize,
    pub download_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            episodes: 6,
            row_bytes: 16 * 1024 * 1024,
            messages: 4000,
            table_rows: 100_000,
            download_bytes: 256 * 1024 * 1024,
        }
    }
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::invalid("output needs a parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|e| Error::internal(e.error.to_string()))?;
    Ok(())
}
