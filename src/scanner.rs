use std::{collections::HashSet, path::PathBuf};

use anyhow::{Context, Result};
use tracing::warn;
use walkdir::WalkDir;

use crate::{config::AppConfig, metadata::read_image_metadata, model::ImageItem};

pub async fn scan_images(root: PathBuf, config: &AppConfig) -> Result<Vec<ImageItem>> {
  let config = config.clone();
  tokio::task::spawn_blocking(move || scan_images_sync(root, config))
    .await
    .context("image scan task failed")?
}

fn scan_images_sync(root: PathBuf, config: AppConfig) -> Result<Vec<ImageItem>> {
  let extensions: HashSet<String> = config
    .supported_extensions
    .iter()
    .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
    .collect();

  let mut walker = WalkDir::new(root).follow_links(false);
  if !config.recursive {
    walker = walker.max_depth(1);
  }

  let mut images = Vec::new();
  for entry in walker.into_iter().filter_map(Result::ok) {
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.into_path();
    let extension = path
      .extension()
      .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
      .unwrap_or_default();
    if !extensions.contains(&extension) {
      continue;
    }

    if let Some(item) = image_item_from_path(path, extension) {
      images.push(item);
    }
  }

  Ok(images)
}

fn image_item_from_path(path: PathBuf, extension: String) -> Option<ImageItem> {
  let metadata = match std::fs::metadata(&path) {
    Ok(metadata) => metadata,
    Err(error) => {
      warn!(
        path = %path.display(),
        %error,
        "skipping inaccessible image during scan"
      );
      return None;
    }
  };
  let dimensions = image::image_dimensions(&path).ok();
  let image_metadata = read_image_metadata(&path);
  let file_name = path
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.display().to_string());

  Some(ImageItem {
    path,
    file_name,
    extension,
    size_bytes: metadata.len(),
    modified: metadata.modified().ok(),
    created: metadata.created().ok(),
    dimensions,
    metadata: image_metadata,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::SystemTime;

  fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
      "gallery-tui-scanner-test-{name}-{}-{}",
      std::process::id(),
      SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
    );
    std::env::temp_dir().join(unique)
  }

  #[test]
  fn missing_path_is_skipped() {
    let item = image_item_from_path(temp_path("missing").join("gone.png"), "png".to_string());

    assert!(item.is_none());
  }

  #[test]
  fn unreadable_image_data_still_keeps_file_metadata() {
    let dir = temp_path("invalid-image");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not-really.png");
    std::fs::write(&path, b"not image data").unwrap();

    let item = image_item_from_path(path, "png".to_string()).unwrap();

    assert_eq!(item.file_name, "not-really.png");
    assert_eq!(item.dimensions, None);

    std::fs::remove_dir_all(&dir).unwrap();
  }
}
