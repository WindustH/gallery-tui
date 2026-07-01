use std::{collections::HashSet, path::PathBuf};

use anyhow::{Context, Result};
use image::{ImageDecoder, ImageReader, metadata::Orientation};
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
  let dimensions = oriented_image_dimensions(&path);
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

fn oriented_image_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
  let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
  let mut decoder = reader.into_decoder().ok()?;
  let dimensions = decoder.dimensions();
  let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
  Some(apply_orientation_to_dimensions(dimensions, orientation))
}

fn apply_orientation_to_dimensions(dimensions: (u32, u32), orientation: Orientation) -> (u32, u32) {
  use Orientation::{Rotate90, Rotate90FlipH, Rotate270, Rotate270FlipH};
  match orientation {
    Rotate90 | Rotate90FlipH | Rotate270 | Rotate270FlipH => (dimensions.1, dimensions.0),
    _ => dimensions,
  }
}
