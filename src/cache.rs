use std::{
  path::{Path, PathBuf},
  time::SystemTime,
};

use anyhow::{Context, Result};
use tokio::fs;

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

async fn collect_render_cache_entries(cache_dir: &Path) -> Result<Vec<CacheEntry>> {
  let mut entries = Vec::new();
  let mut dir = fs::read_dir(cache_dir)
    .await
    .with_context(|| format!("failed to read cache directory {}", cache_dir.display()))?;

  while let Some(entry) = dir
    .next_entry()
    .await
    .with_context(|| format!("failed to scan cache directory {}", cache_dir.display()))?
  {
    let path = entry.path();
    if path.extension().and_then(|value| value.to_str()) != Some("ansi") {
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

  Ok(entries)
}

pub async fn touch_render_cache_entry(cache_path: &Path) {
  let path = render_cache_used_path(cache_path);
  if let Err(error) = fs::write(&path, []).await {
    tracing::warn!(
      cache = %cache_path.display(),
      used_marker = %path.display(),
      %error,
      "failed to update render cache usage marker"
    );
  }
}

fn render_cache_used_path(cache_path: &Path) -> PathBuf {
  let mut path = cache_path.to_path_buf();
  path.set_extension("ansi.used");
  path
}

async fn render_cache_last_used(cache_path: &Path, metadata: &std::fs::Metadata) -> SystemTime {
  if let Ok(used_metadata) = fs::metadata(render_cache_used_path(cache_path)).await {
    if let Ok(modified) = used_metadata.modified() {
      return modified;
    }
  }
  metadata
    .accessed()
    .or_else(|_| metadata.modified())
    .unwrap_or(SystemTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  fn test_cache_dir(name: &str) -> PathBuf {
    let unique = format!(
      "gallery-tui-cache-test-{name}-{}-{}",
      std::process::id(),
      SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
    );
    std::env::temp_dir().join(unique)
  }

  async fn write_bytes(path: &Path, len: usize) {
    fs::write(path, vec![b'x'; len]).await.unwrap();
  }

  #[tokio::test]
  async fn cleanup_ignores_logs_and_non_render_files() {
    let dir = test_cache_dir("ignores");
    fs::create_dir_all(dir.join("logs")).await.unwrap();
    write_bytes(&dir.join("a.ansi"), 80).await;
    write_bytes(&dir.join("b.ansi"), 80).await;
    write_bytes(&dir.join("notes.txt"), 10).await;
    write_bytes(&dir.join("logs").join("run.log"), 200).await;

    let report = enforce_render_cache_limit(&dir, 100).await.unwrap();

    assert_eq!(report.before_bytes, 160);
    assert!(report.after_bytes <= 100);
    assert!(dir.join("notes.txt").exists());
    assert!(dir.join("logs").join("run.log").exists());

    fs::remove_dir_all(&dir).await.unwrap();
  }

  #[tokio::test]
  async fn cleanup_removes_least_recently_used_entry_first() {
    let dir = test_cache_dir("lru");
    fs::create_dir_all(&dir).await.unwrap();
    let old = dir.join("old.ansi");
    let recent = dir.join("recent.ansi");

    write_bytes(&old, 80).await;
    touch_render_cache_entry(&old).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    write_bytes(&recent, 80).await;
    touch_render_cache_entry(&recent).await;

    let report = enforce_render_cache_limit(&dir, 80).await.unwrap();

    assert_eq!(report.removed_files, 1);
    assert!(!old.exists());
    assert!(!render_cache_used_path(&old).exists());
    assert!(recent.exists());

    fs::remove_dir_all(&dir).await.unwrap();
  }

  #[tokio::test]
  async fn clear_render_cache_removes_render_files_and_markers_only() {
    let dir = test_cache_dir("clear");
    fs::create_dir_all(dir.join("logs")).await.unwrap();
    let cache = dir.join("a.ansi");
    write_bytes(&cache, 80).await;
    touch_render_cache_entry(&cache).await;
    write_bytes(&dir.join("logs").join("run.log"), 200).await;
    write_bytes(&dir.join("notes.txt"), 10).await;

    let report = clear_render_cache(&dir).await.unwrap();

    assert_eq!(report.before_bytes, 80);
    assert_eq!(report.after_bytes, 0);
    assert_eq!(report.removed_files, 1);
    assert!(!cache.exists());
    assert!(!render_cache_used_path(&cache).exists());
    assert!(dir.join("logs").join("run.log").exists());
    assert!(dir.join("notes.txt").exists());

    fs::remove_dir_all(&dir).await.unwrap();
  }

  #[tokio::test]
  async fn cleanup_does_nothing_when_under_limit() {
    let dir = test_cache_dir("under-limit");
    fs::create_dir_all(&dir).await.unwrap();
    write_bytes(&dir.join("a.ansi"), 40).await;
    write_bytes(&dir.join("b.ansi"), 50).await;

    let report = enforce_render_cache_limit(&dir, 100).await.unwrap();

    assert_eq!(
      report,
      CacheCleanupReport {
        before_bytes: 90,
        after_bytes: 90,
        removed_files: 0,
        removed_bytes: 0,
      }
    );
    assert!(dir.join("a.ansi").exists());
    assert!(dir.join("b.ansi").exists());

    fs::remove_dir_all(&dir).await.unwrap();
  }
}
