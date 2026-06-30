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
