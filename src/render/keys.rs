//! Hashing helpers for the disk cache and kitty image identifiers.
//!
//! The digests computed here name files in the shared render cache, so their
//! inputs and ordering must stay stable across releases.

use std::{
  path::Path,
  time::{SystemTime, UNIX_EPOCH},
};

use img_tui::{NativeImageConfig, RenderMode, native_image};
use sha2::{Digest, Sha256};

use crate::config::RenderConfig;

/// Disk (and compressed-memory) cache key for one render mode of `path`.
pub(super) fn render_cache_key(
  path: &Path,
  width: u16,
  height: u16,
  config: &RenderConfig,
  native_config: &NativeImageConfig,
  mode: RenderMode,
  session_nonce: u32,
) -> String {
  let mut hasher = Sha256::new();
  hasher.update(path.as_os_str().as_encoded_bytes());
  if let Ok(metadata) = std::fs::metadata(path) {
    hasher.update(metadata.len().to_le_bytes());
    if let Ok(modified) = metadata.modified()
      && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
    {
      hasher.update(duration.as_nanos().to_le_bytes());
    }
  }
  hasher.update(width.to_le_bytes());
  hasher.update(height.to_le_bytes());
  hasher.update(mode.label().as_bytes());
  if mode == RenderMode::Kitty {
    hasher.update([0]);
    hasher.update(session_nonce.to_le_bytes());
  }
  hash_render_config(&mut hasher, config);
  hash_native_config(&mut hasher, native_config);
  for arg in &config.chafa_args {
    hasher.update(arg.as_bytes());
    hasher.update([0]);
  }
  hex::encode(hasher.finalize())
}

fn hash_render_config(hasher: &mut Sha256, config: &RenderConfig) {
  hasher.update(b"render-v7");
  hasher.update([0]);
  hasher.update(config.chafa_bin.as_bytes());
  hasher.update([0]);
  hasher.update(config.chafa_threads.to_le_bytes());
  if let Some(passthrough) = &config.passthrough {
    hasher.update(passthrough.as_bytes());
  }
  hasher.update([0]);
}

fn hash_native_config(hasher: &mut Sha256, config: &NativeImageConfig) {
  let (cell_width, cell_height) = config.cell_pixels.unwrap_or((0, 0));
  hasher.update(cell_width.to_le_bytes());
  hasher.update(cell_height.to_le_bytes());
  hasher.update([0]);
  if let Some(passthrough) = &config.passthrough {
    hasher.update(passthrough.as_bytes());
  }
  hasher.update([0]);
  hasher.update([u8::from(config.kitty_unicode_placeholders)]);
  hasher.update([0]);
}

/// A per-process value mixed into kitty cache keys and image ids so a new
/// session never reuses image ids that an earlier process left in the
/// terminal.
pub(super) fn render_session_nonce() -> u32 {
  let mut hasher = Sha256::new();
  hasher.update(std::process::id().to_le_bytes());
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_nanos();
  hasher.update(now.to_le_bytes());
  let digest = hasher.finalize();
  (u32::from_le_bytes(digest[..4].try_into().unwrap_or_default()) & 0x00ff_ffff).max(1)
}

pub(super) fn kitty_image_id(
  path: &Path,
  width: u16,
  height: u16,
  mode: RenderMode,
  session_nonce: u32,
) -> Option<u32> {
  if mode != RenderMode::Kitty {
    return None;
  }
  let mut hasher = Sha256::new();
  hasher.update(session_nonce.to_le_bytes());
  hasher.update(path.as_os_str().as_encoded_bytes());
  hasher.update(width.to_le_bytes());
  hasher.update(height.to_le_bytes());
  hasher.update(mode.label().as_bytes());
  let digest = hasher.finalize();
  Some(native_image::kitty_image_id(&digest))
}
