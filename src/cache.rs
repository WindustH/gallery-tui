use anyhow::{Context, Result};
pub use img_tui::cache::CacheStats as CacheCleanupReport;
use std::path::{Path, PathBuf};

pub async fn enforce_render_cache_limit(
  cache_dir: &Path,
  max_bytes: u64,
) -> Result<CacheCleanupReport> {
  let session =
    img_tui::cache::paths::session(cache_dir).context("image cache is not initialized")?;
  tokio::task::spawn_blocking(move || session.database.trim(None, max_bytes)).await?
}
pub async fn clear_render_cache(cache_dir: &Path) -> Result<CacheCleanupReport> {
  let session =
    img_tui::cache::paths::session(cache_dir).context("image cache is not initialized")?;
  tokio::task::spawn_blocking(move || session.clear()).await?
}
pub fn svg_cache_dir(cache_dir: &Path) -> PathBuf {
  cache_dir.join("svg")
}
