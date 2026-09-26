//! Reading, normalizing, and rewriting the config files on disk.

use std::{
  io::ErrorKind,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use tokio::fs;

use super::{AppConfig, comments::app_config_toml};
use crate::fs_atomic;

pub async fn write_app_config(path: &Path, config: &AppConfig) -> Result<()> {
  let body = app_config_toml(config)?;
  fs_atomic::write(path, body)
    .await
    .with_context(|| format!("failed to write {}", path.display()))
}

/// A config file format: how it is normalized, validated, and written.
pub(super) trait ConfigFile: Serialize + DeserializeOwned {
  /// Fill in entries added by newer versions of gallery-tui.
  fn normalize(&mut self) {}

  /// Reject a parsed file that cannot be used.
  fn validate(&self) -> Result<(), String> {
    Ok(())
  }

  fn to_toml(&self) -> Result<String> {
    toml::to_string_pretty(self).map_err(Into::into)
  }
}

/// Load `path`, creating it from `default` when missing.
///
/// A file that no longer parses or validates is backed up as
/// `<name>.bak.<pid>.<nanos>` and replaced by `default`. A valid file is
/// normalized and written back only when that changed its content.
pub(super) async fn load_or_create<T: ConfigFile>(path: &Path, default: T) -> Result<T> {
  let body = match fs::read_to_string(path).await {
    Ok(body) => body,
    Err(error) if error.kind() == ErrorKind::NotFound => {
      return write_default(path, default).await;
    }
    Err(error) => return Err(error).with_context(|| format!("failed to read {}", path.display())),
  };
  let Ok(mut parsed) = toml::from_str::<T>(&body) else {
    backup_config_file(path).await?;
    return write_default(path, default).await;
  };
  parsed.normalize();
  if parsed.validate().is_err() {
    backup_config_file(path).await?;
    return write_default(path, default).await;
  }
  let normalized = parsed.to_toml()?;
  write_back_if_toml_changed(path, &body, &normalized).await?;
  Ok(parsed)
}

async fn write_default<T: ConfigFile>(path: &Path, mut default: T) -> Result<T> {
  default.normalize();
  fs_atomic::write(path, default.to_toml()?)
    .await
    .with_context(|| format!("failed to write {}", path.display()))?;
  Ok(default)
}

async fn backup_config_file(path: &Path) -> Result<PathBuf> {
  let backup_path = next_backup_path(path);
  match fs::rename(path, &backup_path).await {
    Ok(()) => {}
    Err(error) if error.kind() == ErrorKind::NotFound => {}
    Err(error) => {
      return Err(error).with_context(|| {
        format!(
          "failed to back up incompatible config {} to {}",
          path.display(),
          backup_path.display()
        )
      });
    }
  }
  Ok(backup_path)
}

fn next_backup_path(path: &Path) -> PathBuf {
  let file_name = path
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("config.toml");
  let stamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_nanos();
  for index in 0..1000 {
    let suffix = if index == 0 {
      format!(".bak.{}.{stamp}", std::process::id())
    } else {
      format!(".bak.{}.{stamp}.{index}", std::process::id())
    };
    let candidate = path.with_file_name(format!("{file_name}{suffix}"));
    if !candidate.exists() {
      return candidate;
    }
  }
  path.with_file_name(format!("{file_name}.bak.{stamp}.overflow"))
}

async fn write_back_if_toml_changed(path: &Path, original: &str, normalized: &str) -> Result<()> {
  if toml_semantic_value(original) != toml_semantic_value(normalized) {
    fs_atomic::write(path, normalized)
      .await
      .with_context(|| format!("failed to update {}", path.display()))?;
  }
  Ok(())
}

fn toml_semantic_value(body: &str) -> Option<toml::Value> {
  toml::from_str(body).ok()
}
