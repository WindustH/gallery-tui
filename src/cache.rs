use std::{
  io::ErrorKind,
  path::{Path, PathBuf},
  time::SystemTime,
};

use anyhow::{Context, Result};
use tokio::fs;

use crate::fs_atomic;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheCleanupReport {
  pub before_bytes: u64,
  pub after_bytes: u64,
  pub removed_files: usize,
  pub removed_bytes: u64,
}

#[derive(Debug)]
struct CacheEntry {
  path: PathBuf,
  size_bytes: u64,
  last_used: SystemTime,
}

pub async fn enforce_render_cache_limit(
  cache_dir: &Path,
  max_bytes: u64,
) -> Result<CacheCleanupReport> {
  if max_bytes == 0 {
    return Ok(CacheCleanupReport::default());
  }

  let mut entries = collect_render_cache_entries(cache_dir).await?;
  let before_bytes = entries.iter().map(|entry| entry.size_bytes).sum::<u64>();
  if before_bytes <= max_bytes {
    return Ok(CacheCleanupReport {
      before_bytes,
      after_bytes: before_bytes,
      removed_files: 0,
      removed_bytes: 0,
    });
  }

  entries.sort_by(|left, right| {
    left
      .last_used
      .cmp(&right.last_used)
      .then_with(|| left.path.cmp(&right.path))
  });

  let mut after_bytes = before_bytes;
  let mut removed_files = 0;
  let mut removed_bytes = 0;

  for entry in entries {
    if after_bytes <= max_bytes {
      break;
    }
    match fs::remove_file(&entry.path).await {
      Ok(()) => {
        let _ = fs::remove_file(render_cache_used_path(&entry.path)).await;
        after_bytes = after_bytes.saturating_sub(entry.size_bytes);
        removed_files += 1;
        removed_bytes += entry.size_bytes;
      }
      Err(error) if error.kind() == ErrorKind::NotFound => {
        after_bytes = after_bytes.saturating_sub(entry.size_bytes);
      }
      Err(error) => {
        tracing::warn!(
          cache = %entry.path.display(),
          %error,
          "failed to remove old render cache entry"
        );
      }
    }
  }

  Ok(CacheCleanupReport {
    before_bytes,
    after_bytes,
    removed_files,
    removed_bytes,
  })
}

pub async fn clear_render_cache(cache_dir: &Path) -> Result<CacheCleanupReport> {
  let entries = collect_render_cache_entries(cache_dir).await?;
  let before_bytes = entries.iter().map(|entry| entry.size_bytes).sum::<u64>();
  let mut removed_files = 0;
  let mut removed_bytes = 0;

  for entry in entries {
    match fs::remove_file(&entry.path).await {
      Ok(()) => {
        let _ = fs::remove_file(render_cache_used_path(&entry.path)).await;
        removed_files += 1;
        removed_bytes += entry.size_bytes;
      }
      Err(error) if error.kind() == ErrorKind::NotFound => {
        removed_bytes += entry.size_bytes;
      }
      Err(error) => {
        tracing::warn!(
          cache = %entry.path.display(),
          %error,
          "failed to remove render cache entry"
        );
      }
    }
  }

  Ok(CacheCleanupReport {
    before_bytes,
    after_bytes: before_bytes.saturating_sub(removed_bytes),
    removed_files,
    removed_bytes,
  })
}

/// Render cache files: `<key>.ansi` renders in `cache_dir` and rasterized
/// SVGs in `cache_dir/svg`.
async fn collect_render_cache_entries(cache_dir: &Path) -> Result<Vec<CacheEntry>> {
  let mut entries = Vec::new();
  collect_cache_files(cache_dir, "ansi", &mut entries)
    .await
    .with_context(|| format!("failed to scan cache directory {}", cache_dir.display()))?;
  let svg_dir = svg_cache_dir(cache_dir);
  match collect_cache_files(&svg_dir, "png", &mut entries).await {
    Ok(()) => {}
    Err(error) if error.kind() == ErrorKind::NotFound => {}
    Err(error) => tracing::warn!(dir = %svg_dir.display(), %error, "failed to scan SVG cache"),
  }
  Ok(entries)
}

async fn collect_cache_files(
  dir: &Path,
  extension: &str,
  entries: &mut Vec<CacheEntry>,
) -> std::io::Result<()> {
  let mut dir = fs::read_dir(dir).await?;
  while let Some(entry) = dir.next_entry().await? {
    let path = entry.path();
    if path.extension().and_then(|value| value.to_str()) != Some(extension) {
      continue;
    }

    let metadata = match entry.metadata().await {
      Ok(metadata) => metadata,
      Err(error) => {
        tracing::warn!(cache = %path.display(), %error, "failed to stat render cache entry");
        continue;
      }
    };
    if !metadata.is_file() {
      continue;
    }

    let last_used = render_cache_last_used(&path, &metadata).await;
    entries.push(CacheEntry {
      path,
      size_bytes: metadata.len(),
      last_used,
    });
  }
  Ok(())
}

/// Directory holding PNG rasterizations of SVG files.
pub fn svg_cache_dir(cache_dir: &Path) -> PathBuf {
  cache_dir.join("svg")
}

pub async fn touch_render_cache_entry(cache_path: &Path) {
  if fs::metadata(cache_path).await.is_err() {
    return;
  }
  let path = render_cache_used_path(cache_path);
  if let Err(error) = fs_atomic::write(&path, []).await {
    tracing::warn!(
      cache = %cache_path.display(),
      used_marker = %path.display(),
      %error,
      "failed to update render cache usage marker"
    );
  }
}

/// LRU marker next to a cache file: `<file name>.used`.
fn render_cache_used_path(cache_path: &Path) -> PathBuf {
  let mut name = cache_path.file_name().unwrap_or_default().to_os_string();
  name.push(".used");
  cache_path.with_file_name(name)
}

async fn render_cache_last_used(cache_path: &Path, metadata: &std::fs::Metadata) -> SystemTime {
  if let Ok(used_metadata) = fs::metadata(render_cache_used_path(cache_path)).await
    && let Ok(modified) = used_metadata.modified()
  {
    return modified;
  }
  metadata
    .accessed()
    .or_else(|_| metadata.modified())
    .unwrap_or(SystemTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  #[tokio::test]
  async fn clear_removes_renders_svg_rasters_and_markers_only() {
    let dir = std::env::temp_dir().join(format!(
      "gallery-tui-cache-{}-{}",
      std::process::id(),
      SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
    ));
    let svg = svg_cache_dir(&dir);
    fs::create_dir_all(&svg).unwrap();
    fs::write(dir.join("a.ansi"), b"12345").unwrap();
    fs::write(dir.join("a.ansi.used"), b"").unwrap();
    fs::write(svg.join("b.png"), b"123").unwrap();
    fs::write(svg.join("b.png.used"), b"").unwrap();
    fs::write(dir.join("notes.txt"), b"keep").unwrap();

    let report = clear_render_cache(&dir).await.unwrap();
    let mut left: Vec<_> = walkdir::WalkDir::new(&dir)
      .into_iter()
      .filter_map(Result::ok)
      .filter(|entry| entry.file_type().is_file())
      .map(|entry| entry.file_name().to_string_lossy().into_owned())
      .collect();
    left.sort();
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(report.removed_files, 2);
    assert_eq!(report.removed_bytes, 8);
    assert_eq!(left, ["notes.txt"]);
  }
}
