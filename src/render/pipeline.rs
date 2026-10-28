//! One render job: try each render mode in order, reading the compressed
//! memory and disk cache tiers before rendering from the source image.

use std::path::{Path, PathBuf};

use img_tui::{
  EncodedProtocolImage, NativeImageConfig, ProtocolImage, ProtocolImageSpec, RenderMode,
  native_image,
};
use ratatui::widgets::Paragraph;
use tokio::fs;
use tracing::{debug, warn};

use super::{
  RenderedImage,
  cache_file::{CacheHeader, RenderedBytes, decode_cache_file, encode_cache_file},
  keys::{kitty_image_id, render_cache_key},
};
use crate::config::RenderConfig;
use img_tui::cache::{MemoryCache as LruCache, paths as files};

/// State shared by every render job of one `RenderStore`.
pub(super) struct RenderContext {
  pub(super) cache_dir: PathBuf,
  pub(super) config: RenderConfig,
  pub(super) native_config: NativeImageConfig,
  pub(super) modes: Vec<RenderMode>,
  pub(super) compressed_memory: LruCache<String, Vec<u8>>,
  pub(super) session_nonce: u32,
}

type PreparedNative = Option<Result<native_image::PreparedNativeImage, String>>;

impl RenderContext {
  pub(super) fn clear_compressed_memory(&self) {
    self.compressed_memory.clear();
  }
  fn compressed_get(&self, key: &str) -> Option<Vec<u8>> {
    self.compressed_memory.get(key).map(|v| (*v).clone())
  }
  fn compressed_insert(&self, key: String, bytes: Vec<u8>) {
    let size = bytes.len() as u64;
    self.compressed_memory.insert(key, bytes, size);
  }
  fn compressed_remove(&self, key: &str) {
    self.compressed_memory.remove(key);
  }

  /// Render `image_path` into a `width` x `height` cell box, trying each
  /// configured mode until one succeeds.
  pub(super) async fn render(
    &self,
    image_path: &Path,
    width: u16,
    height: u16,
  ) -> Result<RenderedImage, String> {
    // SVGs cannot be decoded by the `image` crate or Chafa: rasterize to a
    // cached PNG (oversampled 2x for crisp downscaling) and render that.
    let rasterized;
    let image_path = if crate::svg::is_svg(image_path) {
      let (cell_width, cell_height) = self.native_config.cell_pixels.unwrap_or((8, 16));
      let oversample = 2;
      let target = (
        u32::from(width.max(1)) * u32::from(cell_width.max(1)) * oversample,
        u32::from(height.max(1)) * u32::from(cell_height.max(1)) * oversample,
      );
      rasterized = crate::svg::ensure_rasterized(image_path, target, &self.cache_dir).await?;
      rasterized.as_path()
    } else {
      image_path
    };

    let mut errors = Vec::new();
    // Protocol modes share one decoded and resized source image.
    let mut prepared_native = None;
    for &mode in &self.modes {
      let job = ModeJob::new(self, image_path, width, height, mode);
      match job.run(&mut prepared_native).await {
        Ok(rendered) => {
          debug!(path = %image_path.display(), mode = mode.label(), "render succeeded");
          return Ok(rendered);
        }
        Err(error) => {
          warn!(path = %image_path.display(), mode = mode.label(), error, "render mode failed");
          errors.push(format!("{}: {error}", mode.label()));
        }
      }
    }
    Err(errors.join("; "))
  }
}

/// A render of one image in one mode, with its cache identity.
struct ModeJob<'a> {
  ctx: &'a RenderContext,
  image_path: &'a Path,
  cache_key: String,
  cache_path: PathBuf,
  header: CacheHeader,
}

impl<'a> ModeJob<'a> {
  fn new(
    ctx: &'a RenderContext,
    image_path: &'a Path,
    width: u16,
    height: u16,
    mode: RenderMode,
  ) -> Self {
    let cache_key = render_cache_key(
      image_path,
      width,
      height,
      &ctx.config,
      &ctx.native_config,
      mode,
      ctx.session_nonce,
    );
    let cache_path = ctx.cache_dir.join(format!("{cache_key}.ansi"));
    let image_id = kitty_image_id(image_path, width, height, mode, ctx.session_nonce);
    let placement_id = if ctx.native_config.kitty_unicode_placeholders || mode != RenderMode::Kitty
    {
      None
    } else {
      image_id
    };
    Self {
      ctx,
      image_path,
      cache_key,
      cache_path,
      header: CacheHeader {
        width,
        height,
        cell_pixels: ctx.native_config.cell_pixels,
        mode,
        image_id,
        placement_id,
      },
    }
  }

  fn mode(&self) -> RenderMode {
    self.header.mode
  }

  async fn run(self, prepared_native: &mut PreparedNative) -> Result<RenderedImage, String> {
    let ctx = self.ctx;
    if let Some(bytes) = self.read_compressed_memory().await {
      return decode_rendered(bytes, &self.header, &ctx.native_config);
    }
    if let Some(bytes) = self.read_disk().await {
      return decode_rendered(bytes, &self.header, &ctx.native_config);
    }

    debug!(
      path = %self.image_path.display(),
      mode = self.mode().label(),
      cache_tier = "compute",
      "render cache miss"
    );
    let can_write_disk_cache = self.ensure_cache_dir().await;
    let bytes = if self.mode().is_protocol() {
      self.render_protocol(prepared_native).await?
    } else {
      RenderedBytes {
        data: run_chafa(self.image_path, &self.header, &ctx.config).await?,
        refresh: None,
      }
    };
    self.store(&bytes, can_write_disk_cache).await;
    decode_rendered(bytes, &self.header, &ctx.native_config)
  }

  async fn read_compressed_memory(&self) -> Option<RenderedBytes> {
    let ctx = self.ctx;
    let bytes = ctx.compressed_get(&self.cache_key)?;
    match decode_cache_file(&bytes, &self.header).await {
      Ok(decoded) => {
        debug!(
          path = %self.image_path.display(),
          mode = self.mode().label(),
          cache_tier = "compressed-memory",
          "render cache hit"
        );
        Some(decoded.payload)
      }
      Err(error) => {
        debug!(cache = %self.cache_path.display(), error, "ignoring stale compressed memory render cache");
        ctx.compressed_remove(&self.cache_key);
        None
      }
    }
  }

  async fn read_disk(&self) -> Option<RenderedBytes> {
    let ctx = self.ctx;
    let bytes = files::read(&self.cache_path).await.ok()?;
    let decoded = match decode_cache_file(&bytes, &self.header).await {
      Ok(decoded) => decoded,
      Err(error) => {
        debug!(cache = %self.cache_path.display(), error, "ignoring stale render cache");
        return None;
      }
    };
    debug!(
      path = %self.image_path.display(),
      mode = self.mode().label(),
      cache_tier = "disk",
      "render cache hit"
    );
    if decoded.should_rewrite {
      match self.encode(&decoded.payload).await {
        Ok(cached) => {
          ctx.compressed_insert(self.cache_key.clone(), cached.clone());
          if let Err(error) = files::write(&self.cache_path, &cached).await {
            warn!(
              cache = %self.cache_path.display(),
              %error,
              "failed to rewrite render cache with current compression"
            );
          }
        }
        Err(error) => {
          warn!(
            cache = %self.cache_path.display(),
            %error,
            "failed to encode compressed render cache rewrite"
          );
        }
      }
    } else {
      ctx.compressed_insert(self.cache_key.clone(), bytes);
    }

    Some(decoded.payload)
  }

  async fn ensure_cache_dir(&self) -> bool {
    let Some(parent) = self.cache_path.parent() else {
      return true;
    };
    match fs::create_dir_all(parent).await {
      Ok(()) => true,
      Err(error) => {
        warn!(
          cache_dir = %parent.display(),
          %error,
          "failed to create render cache directory"
        );
        false
      }
    }
  }

  async fn encode(&self, payload: &RenderedBytes) -> Result<Vec<u8>, String> {
    encode_cache_file(
      payload,
      &self.header,
      self.ctx.config.cache_compression_level,
      self.ctx.config.cache_compression_threads,
    )
    .await
  }

  /// Save a fresh render to the compressed memory tier and, when possible,
  /// the disk tier.
  async fn store(&self, bytes: &RenderedBytes, write_disk: bool) {
    let ctx = self.ctx;
    let cached = match self.encode(bytes).await {
      Ok(cached) => cached,
      Err(error) => {
        warn!(
          cache = %self.cache_path.display(),
          %error,
          "failed to encode render cache"
        );
        return;
      }
    };
    ctx.compressed_insert(self.cache_key.clone(), cached.clone());
    if !write_disk {
      return;
    }
    match files::write(&self.cache_path, &cached).await {
      Ok(()) => {}
      Err(error) => {
        warn!(
          cache = %self.cache_path.display(),
          %error,
          "failed to write render cache"
        );
      }
    }
  }

  async fn render_protocol(
    &self,
    prepared_native: &mut PreparedNative,
  ) -> Result<RenderedBytes, String> {
    let ctx = self.ctx;
    let header = &self.header;
    let prepared = match prepared_native {
      Some(prepared) => prepared,
      None => prepared_native.insert(
        native_image::prepare(
          self.image_path,
          header.width,
          header.height,
          header.cell_pixels,
        )
        .await
        .map_err(|err| err.to_string()),
      ),
    };
    let prepared = prepared.as_ref().map_err(Clone::clone)?;
    native_image::encode_protocol(prepared, &protocol_spec(header), &ctx.native_config)
      .await
      .map(|encoded| RenderedBytes {
        data: encoded.data,
        refresh: encoded.refresh,
      })
      .map_err(|err| err.to_string())
  }
}

async fn run_chafa(
  image_path: &Path,
  header: &CacheHeader,
  config: &RenderConfig,
) -> Result<Vec<u8>, String> {
  img_tui::chafa::run(
    image_path,
    header.width,
    header.height,
    &img_tui::chafa::ChafaConfig {
      chafa_bin: &config.chafa_bin,
      chafa_args: &config.chafa_args,
      chafa_threads: config.chafa_threads,
    },
    header.mode,
  )
  .await
}

fn decode_rendered(
  bytes: RenderedBytes,
  header: &CacheHeader,
  native_config: &NativeImageConfig,
) -> Result<RenderedImage, String> {
  let mode = header.mode;
  if !mode.is_protocol() {
    let text = img_tui::chafa::decode(&bytes.data)?;
    let size = text
      .lines
      .iter()
      .flat_map(|line| line.spans.iter())
      .map(|span| span.content.len() as u64)
      .sum();
    return Ok(RenderedImage::Symbols {
      paragraph: Box::new(Paragraph::new(text)),
      size,
    });
  }

  let encoded = EncodedProtocolImage {
    data: bytes.data,
    refresh: bytes.refresh,
  };
  ProtocolImage::from_encoded(encoded, &protocol_spec(header), native_config)
    .map(RenderedImage::Protocol)
    .map_err(|err| err.to_string())
}

/// How a protocol render described by `header` is encoded and read back.
fn protocol_spec(header: &CacheHeader) -> ProtocolImageSpec {
  ProtocolImageSpec {
    image_id: header.image_id,
    placement_id: header.placement_id,
    ..ProtocolImageSpec::new(header.mode, header.width, header.height)
  }
}
