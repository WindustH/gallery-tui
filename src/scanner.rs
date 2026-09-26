use std::{
  collections::HashSet,
  num::NonZeroUsize,
  panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
  path::{Path, PathBuf},
  sync::atomic::{AtomicUsize, Ordering},
  thread,
};

use anyhow::{Context, Result};
use image::{ImageDecoder, ImageReader, metadata::Orientation};
use tracing::{debug, warn};
use walkdir::WalkDir;

use crate::{
  config::AppConfig,
  metadata::read_image_metadata,
  model::{ImageItem, file_extension, file_label},
};

/// Upper bound on threads reading image headers and EXIF data.
const MAX_SCAN_WORKERS: usize = 8;

pub async fn scan_images(root: PathBuf, config: &AppConfig) -> Result<Vec<ImageItem>> {
  let extensions: HashSet<String> = config
    .supported_extensions
    .iter()
    .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
    .collect();
  let recursive = config.recursive;
  tokio::task::spawn_blocking(move || {
    let paths = image_paths(&root, recursive, &extensions);
    read_image_items(paths)
  })
  .await
  .context("image scan task failed")
}

/// Files under `root` with a supported extension, in directory walk order.
///
/// Symlinked directories are not followed, so a link cycle cannot loop the
/// scan, but symlinks to image files are listed like the files themselves.
fn image_paths(root: &Path, recursive: bool, extensions: &HashSet<String>) -> Vec<PathBuf> {
  let mut walker = WalkDir::new(root).follow_links(false);
  if !recursive {
    walker = walker.max_depth(1);
  }
  walker
    .into_iter()
    .filter_map(|entry| {
      entry
        .inspect_err(|error| debug!(%error, "skipping unreadable entry during scan"))
        .ok()
    })
    .filter(|entry| !entry.file_type().is_dir())
    .map(walkdir::DirEntry::into_path)
    .filter(|path| extensions.contains(&file_extension(path)))
    .collect()
}

/// Read file metadata, image dimensions, and EXIF tags for `paths` on a few
/// threads, keeping the input order.
fn read_image_items(paths: Vec<PathBuf>) -> Vec<ImageItem> {
  let workers = thread::available_parallelism()
    .map_or(1, NonZeroUsize::get)
    .clamp(1, MAX_SCAN_WORKERS)
    .min(paths.len());
  if workers <= 1 {
    return paths.into_iter().filter_map(image_item_from_path).collect();
  }

  let next = AtomicUsize::new(0);
  let read: Vec<Vec<(usize, ImageItem)>> = thread::scope(|scope| {
    let handles: Vec<_> = (0..workers)
      .map(|_| {
        scope.spawn(|| {
          let mut read = Vec::new();
          loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(path) = paths.get(index) else {
              break;
            };
            if let Some(item) = image_item_from_path(path.clone()) {
              read.push((index, item));
            }
          }
          read
        })
      })
      .collect();
    handles
      .into_iter()
      .map(|handle| handle.join().unwrap_or_else(|panic| resume_unwind(panic)))
      .collect()
  });
  let mut items: Vec<_> = read.into_iter().flatten().collect();
  items.sort_unstable_by_key(|(index, _)| *index);
  items.into_iter().map(|(_, item)| item).collect()
}

fn image_item_from_path(path: PathBuf) -> Option<ImageItem> {
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
  // Skips symlinks to directories and special files such as FIFOs, which
  // would block when opened.
  if !metadata.is_file() {
    return None;
  }
  // A malformed file must not abort the whole scan if a decoder panics.
  let (dimensions, image_metadata) = match catch_unwind(AssertUnwindSafe(|| {
    (oriented_image_dimensions(&path), read_image_metadata(&path))
  })) {
    Ok(read) => read,
    Err(_) => {
      warn!(path = %path.display(), "image header reader panicked; skipping file details");
      (None, Vec::new())
    }
  };

  Some(ImageItem {
    file_name: file_label(&path),
    extension: file_extension(&path),
    path,
    size_bytes: metadata.len(),
    modified: metadata.modified().ok(),
    created: metadata.created().ok(),
    dimensions,
    metadata: image_metadata,
  })
}

fn oriented_image_dimensions(path: &Path) -> Option<(u32, u32)> {
  if crate::svg::is_svg(path) {
    return crate::svg::svg_dimensions(path).ok();
  }
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

#[cfg(all(test, unix))]
mod tests {
  use std::{fs, os::unix::fs::symlink};

  use super::*;

  fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
      "gallery-tui-scan-{name}-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
  }

  #[test]
  fn scan_lists_symlinked_images_without_following_directories() {
    let dir = temp_dir("symlinks");
    fs::write(dir.join("real.png"), b"not really a png").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub").join("nested.png"), b"x").unwrap();
    symlink(dir.join("real.png"), dir.join("link.png")).unwrap();
    symlink(dir.join("missing.png"), dir.join("broken.png")).unwrap();
    symlink(dir.join("sub"), dir.join("dir-link.png")).unwrap();
    // A directory cycle must not loop a recursive scan.
    symlink(&dir, dir.join("sub").join("cycle")).unwrap();

    let extensions = HashSet::from(["png".to_string()]);
    let mut names: Vec<_> = read_image_items(image_paths(&dir, true, &extensions))
      .into_iter()
      .map(|item| item.file_name)
      .collect();
    names.sort();
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(names, ["link.png", "nested.png", "real.png"]);
  }
}
