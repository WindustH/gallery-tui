use std::{
  fs::{File, OpenOptions},
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use chrono::Local;
use tracing_subscriber::fmt;

pub fn init(cache_dir: &Path) -> Result<PathBuf> {
  let log_dir = cache_dir.join("logs");
  std::fs::create_dir_all(&log_dir)
    .with_context(|| format!("failed to create {}", log_dir.display()))?;
  let started = Local::now().format("%Y%m%d-%H%M%S").to_string();
  let (log_path, file) = create_log_file(&log_dir, &started)?;

  fmt()
    .with_writer(file)
    .with_ansi(false)
    .with_target(true)
    .with_thread_ids(true)
    .with_level(true)
    .init();

  Ok(log_path)
}

fn create_log_file(log_dir: &Path, started: &str) -> Result<(PathBuf, File)> {
  let nonce = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_nanos();
  for index in 0..1000 {
    let suffix = if index == 0 {
      format!("{}-{nonce}", std::process::id())
    } else {
      format!("{}-{nonce}-{index}", std::process::id())
    };
    let path = log_dir.join(format!("{started}-{suffix}.log"));
    match OpenOptions::new().write(true).create_new(true).open(&path) {
      Ok(file) => return Ok((path, file)),
      Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
      Err(error) => {
        return Err(error).with_context(|| format!("failed to create {}", path.display()));
      }
    }
  }
  let path = log_dir.join(format!(
    "{started}-{}-{nonce}-overflow.log",
    std::process::id()
  ));
  let file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(&path)
    .with_context(|| format!("failed to create {}", path.display()))?;
  Ok((path, file))
}
