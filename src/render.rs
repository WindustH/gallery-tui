use std::{
  collections::{HashMap, HashSet},
  io::{Cursor, Write as IoWrite},
  path::{Path, PathBuf},
  process::Command,
  sync::Arc,
};

use ansi_to_tui::IntoText;
use sha2::{Digest, Sha256};
use tokio::{
  fs,
  sync::{OwnedSemaphorePermit, Semaphore, mpsc},
};
use tracing::{debug, warn};

use crate::{
  cache,
  capability::RenderMode,
  config::RenderConfig,
  event::{AsyncEvent, RenderOutcome, RenderedImage},
  model::ImageItem,
  native_image::{self, NativeImageConfig},
};

pub struct RenderStore {
  cache_dir: PathBuf,
  config: RenderConfig,
  native_config: NativeImageConfig,
  modes: Vec<RenderMode>,
  memory: HashMap<String, RenderedImage>,
  failures: HashMap<String, String>,
  in_flight: HashSet<String>,
  semaphore: Arc<Semaphore>,
  preload_semaphore: Arc<Semaphore>,
}

struct RenderPermits {
  _global: OwnedSemaphorePermit,
  _preload: Option<OwnedSemaphorePermit>,
}

impl RenderStore {
  pub fn new(
    cache_dir: PathBuf,
    config: RenderConfig,
    native_config: NativeImageConfig,
    modes: Vec<RenderMode>,
  ) -> Self {
    let max_concurrent = config.max_concurrent.max(1);
    let max_preloads = max_concurrent.saturating_sub(1);
    Self {
      cache_dir,
      config,
      native_config,
      modes,
      memory: HashMap::new(),
      failures: HashMap::new(),
      in_flight: HashSet::new(),
      semaphore: Arc::new(Semaphore::new(max_concurrent)),
      preload_semaphore: Arc::new(Semaphore::new(max_preloads)),
    }
  }

  pub fn get(&self, item: &ImageItem, width: u16, height: u16) -> Option<&RenderedImage> {
    let key = self.cache_key(item, width, height);
    self.memory.get(&key)
  }

  pub fn failure(&self, item: &ImageItem, width: u16, height: u16) -> Option<&str> {
    let key = self.cache_key(item, width, height);
    self.failures.get(&key).map(String::as_str)
  }

  pub fn request(
    &mut self,
    item: &ImageItem,
    width: u16,
    height: u16,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
  ) {
    self.request_with_permits(item, width, height, tx, None);
  }

  fn request_with_permits(
    &mut self,
    item: &ImageItem,
    width: u16,
    height: u16,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
    permits: Option<RenderPermits>,
  ) {
    if width == 0 || height == 0 {
      return;
    }
    let cache_key = self.cache_key(item, width, height);
    if self.memory.contains_key(&cache_key)
      || self.failures.contains_key(&cache_key)
      || self.in_flight.contains(&cache_key)
    {
      return;
    }

    self.in_flight.insert(cache_key.clone());
    let path = item.path.clone();
    let cache_dir = self.cache_dir.clone();
    let config = self.config.clone();
    let native_config = self.native_config.clone();
    let modes = self.modes.clone();
    let tx = tx.clone();
    let semaphore = self.semaphore.clone();

    tokio::spawn(async move {
      let result = render_with_fallbacks(
        path,
        cache_dir,
        width,
        height,
        config,
        native_config,
        modes,
        semaphore,
        permits,
      )
      .await;
      let _ = tx.send(AsyncEvent::Render(RenderOutcome { cache_key, result }));
    });
  }

  pub fn preload(
    &mut self,
    item: &ImageItem,
    width: u16,
    height: u16,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
  ) {
    if self.in_flight.len() >= self.config.max_concurrent.max(1) {
      return;
    }
    let Some(permits) = self.try_preload_permits() else {
      return;
    };
    self.request_with_permits(item, width, height, tx, Some(permits));
  }

  fn try_preload_permits(&self) -> Option<RenderPermits> {
    let preload = self.preload_semaphore.clone().try_acquire_owned().ok()?;
    let global = self.semaphore.clone().try_acquire_owned().ok()?;
    Some(RenderPermits {
      _global: global,
      _preload: Some(preload),
    })
  }

  pub fn finish(&mut self, outcome: RenderOutcome) -> Option<String> {
    self.in_flight.remove(&outcome.cache_key);
    match outcome.result {
      Ok(text) => {
        self.failures.remove(&outcome.cache_key);
        self.memory.insert(outcome.cache_key, text);
        None
      }
      Err(error) => {
        self
          .failures
          .insert(outcome.cache_key.clone(), error.clone());
        Some(format!("render failed: {error}"))
      }
    }
  }

  fn cache_key(&self, item: &ImageItem, width: u16, height: u16) -> String {
    let mut hasher = Sha256::new();
    hasher.update(item.path.to_string_lossy().as_bytes());
    hasher.update(item.size_bytes.to_le_bytes());
    hasher.update(item.modified_key().to_le_bytes());
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hash_render_config(&mut hasher, &self.config);
    hash_native_config(&mut hasher, &self.native_config);
    for arg in &self.config.chafa_args {
      hasher.update(arg.as_bytes());
      hasher.update([0]);
    }
    for mode in &self.modes {
      hasher.update(mode.label().as_bytes());
      hasher.update([0]);
    }
    hex::encode(hasher.finalize())
  }
}

#[allow(clippy::too_many_arguments)]
async fn render_with_fallbacks(
  image_path: PathBuf,
  cache_dir: PathBuf,
  width: u16,
  height: u16,
  config: RenderConfig,
  native_config: NativeImageConfig,
  modes: Vec<RenderMode>,
  semaphore: Arc<Semaphore>,
  permits: Option<RenderPermits>,
) -> Result<RenderedImage, String> {
  let _permits = match permits {
    Some(permits) => permits,
    None => RenderPermits {
      _global: semaphore
        .acquire_owned()
        .await
        .map_err(|err| err.to_string())?,
      _preload: None,
    },
  };
  let mut errors = Vec::new();
  let mut prepared_native = None;
  for mode in modes {
    let cache_path = cache_dir.join(format!(
      "{}.ansi",
      render_cache_key(&image_path, width, height, &config, &native_config, mode)
    ));
    let rendered = if mode.is_protocol() {
      render_or_read_cache(
        image_path.clone(),
        cache_path,
        width,
        height,
        config.clone(),
        native_config.clone(),
        mode,
        Some(&mut prepared_native),
      )
      .await
    } else {
      render_or_read_cache(
        image_path.clone(),
        cache_path,
        width,
        height,
        config.clone(),
        native_config.clone(),
        mode,
        None,
      )
      .await
    };
    match rendered {
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

#[allow(clippy::too_many_arguments)]
async fn render_or_read_cache(
  image_path: PathBuf,
  cache_path: PathBuf,
  width: u16,
  height: u16,
  config: RenderConfig,
  native_config: NativeImageConfig,
  mode: RenderMode,
  prepared_native: Option<&mut Option<Result<native_image::PreparedNativeImage, String>>>,
) -> Result<RenderedImage, String> {
  let image_id = kitty_image_id(&image_path, width, height, mode);
  if let Ok(bytes) = fs::read(&cache_path).await {
    match decode_cache_file(
      &bytes,
      width,
      height,
      native_config.cell_pixels,
      mode,
      image_id,
    )
    .await
    {
      Ok(decoded) => {
        if decoded.should_rewrite {
          match encode_cache_file(
            &decoded.payload,
            width,
            height,
            native_config.cell_pixels,
            mode,
            decoded.image_id,
            &config,
          )
          .await
          {
            Ok(cached) => {
              if let Err(error) = fs::write(&cache_path, cached).await {
                warn!(
                  cache = %cache_path.display(),
                  %error,
                  "failed to rewrite render cache with current compression"
                );
              }
            }
            Err(error) => {
              warn!(
                cache = %cache_path.display(),
                %error,
                "failed to encode compressed render cache rewrite"
              );
            }
          }
        }
        cache::touch_render_cache_entry(&cache_path).await;
        return decode_rendered(decoded.payload, mode, &native_config, decoded.image_id);
      }
      Err(error) => {
        debug!(cache = %cache_path.display(), error, "ignoring stale render cache");
      }
    }
  }

  if let Some(parent) = cache_path.parent() {
    fs::create_dir_all(parent)
      .await
      .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
  }

  let bytes = if mode.is_protocol() {
    match prepared_native {
      Some(prepared_native) => {
        if prepared_native.is_none() {
          *prepared_native = Some(
            native_image::prepare(&image_path, width, height, native_config.cell_pixels)
              .await
              .map_err(|err| err.to_string()),
          );
        }
        match prepared_native
          .as_ref()
          .expect("prepared native image result exists")
        {
          Ok(prepared) => native_image::render_prepared(prepared, mode, &native_config, image_id)
            .await
            .map_err(|err| err.to_string())?,
          Err(error) => return Err(error.clone()),
        }
      }
      None => native_image::render(&image_path, width, height, mode, &native_config, image_id)
        .await
        .map_err(|err| err.to_string())?,
    }
  } else {
    run_chafa(&image_path, width, height, &config, mode).await?
  };
  let cached = encode_cache_file(
    &bytes,
    width,
    height,
    native_config.cell_pixels,
    mode,
    image_id,
    &config,
  )
  .await
  .map_err(|err| format!("failed to encode cache {}: {err}", cache_path.display()))?;
  fs::write(&cache_path, cached)
    .await
    .map_err(|err| format!("failed to write cache {}: {err}", cache_path.display()))?;
  cache::touch_render_cache_entry(&cache_path).await;
  decode_rendered(bytes, mode, &native_config, image_id)
}

async fn run_chafa(
  image_path: &Path,
  width: u16,
  height: u16,
  config: &RenderConfig,
  mode: RenderMode,
) -> Result<Vec<u8>, String> {
  if mode.is_protocol() {
    return Err(format!(
      "{} must be rendered by native image driver, not chafa",
      mode.label()
    ));
  }

  let mut command = Command::new(&config.chafa_bin);
  let mut args: Vec<String> = config
    .chafa_args
    .iter()
    .filter(|arg| {
      !arg.starts_with("--format=")
        && !arg.starts_with("--colors=")
        && !arg.starts_with("--symbols=")
        && !arg.starts_with("--passthrough=")
        && !arg.starts_with("--probe=")
        && !arg.starts_with("--relative=")
    })
    .cloned()
    .collect();

  args.push(format!("--format={}", mode.chafa_format()));
  args.push("--probe=off".to_string());
  args.push("--relative=off".to_string());
  args.push("--passthrough=none".to_string());
  if !args.iter().any(|arg| arg.starts_with("--scale=")) {
    args.push("--scale=max".to_string());
  }
  if config.chafa_threads > 0
    && !config
      .chafa_args
      .iter()
      .any(|arg| arg.starts_with("--threads="))
  {
    args.push(format!("--threads={}", config.chafa_threads));
  }
  match mode {
    RenderMode::Symbols => {
      for arg in &config.chafa_args {
        if arg.starts_with("--colors=") || arg.starts_with("--symbols=") {
          args.push(arg.clone());
        }
      }
    }
    RenderMode::Ascii => {
      args.push("--colors=none".to_string());
      args.push("--symbols=ascii".to_string());
    }
    _ => {}
  }

  command
    .args(args)
    .arg("--size")
    .arg(format!("{width}x{height}"))
    .arg(image_path);

  let chafa_bin = config.chafa_bin.clone();
  let output = tokio::task::spawn_blocking(move || command.output())
    .await
    .map_err(|err| format!("chafa worker failed: {err}"))?
    .map_err(|err| format!("failed to run {chafa_bin}: {err}"))?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(format!(
      "{} exited with {}: {}",
      config.chafa_bin,
      output.status,
      stderr.trim()
    ));
  }
  Ok(output.stdout)
}

fn decode_rendered(
  bytes: Vec<u8>,
  mode: RenderMode,
  native_config: &NativeImageConfig,
  image_id: Option<u32>,
) -> Result<RenderedImage, String> {
  if mode.is_protocol() {
    let fingerprint = render_fingerprint(&bytes);
    let data = String::from_utf8(bytes).map_err(|err| err.to_string())?;
    let erase = native_image::erase_sequence(mode, native_config.passthrough.as_deref(), image_id);
    Ok(RenderedImage::Protocol {
      mode,
      data,
      fingerprint,
      erase,
    })
  } else {
    let text = bytes.into_text().map_err(|err| err.to_string())?;
    Ok(RenderedImage::Symbols { mode, text })
  }
}

fn render_cache_key(
  path: &Path,
  width: u16,
  height: u16,
  config: &RenderConfig,
  native_config: &NativeImageConfig,
  mode: RenderMode,
) -> String {
  let mut hasher = Sha256::new();
  hasher.update(path.to_string_lossy().as_bytes());
  if let Ok(metadata) = std::fs::metadata(path) {
    hasher.update(metadata.len().to_le_bytes());
    if let Ok(modified) = metadata.modified()
      && let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH)
    {
      hasher.update(duration.as_nanos().to_le_bytes());
    }
  }
  hasher.update(width.to_le_bytes());
  hasher.update(height.to_le_bytes());
  hasher.update(mode.label().as_bytes());
  hash_render_config(&mut hasher, config);
  hash_native_config(&mut hasher, native_config);
  for arg in &config.chafa_args {
    hasher.update(arg.as_bytes());
    hasher.update([0]);
  }
  hex::encode(hasher.finalize())
}

const CACHE_MAGIC: &str = "gallery-tui-cache-v6";
const LEGACY_RAW_CACHE_MAGIC: &str = "gallery-tui-cache-v4";

struct DecodedCacheFile {
  payload: Vec<u8>,
  image_id: Option<u32>,
  should_rewrite: bool,
}

async fn encode_cache_file(
  payload: &[u8],
  width: u16,
  height: u16,
  cell_pixels: Option<(u16, u16)>,
  mode: RenderMode,
  image_id: Option<u32>,
  config: &RenderConfig,
) -> Result<Vec<u8>, String> {
  let compression_level = config.cache_compression_level;
  let compression_threads = config.cache_compression_threads;
  let plain_len = payload.len();
  let payload = payload.to_vec();
  let compressed = tokio::task::spawn_blocking(move || {
    compress_zstd(payload, compression_level, compression_threads)
  })
  .await
  .map_err(|err| format!("zstd compression worker failed: {err}"))?
  .map_err(|err| format!("zstd compression failed: {err}"))?;

  let (cell_width, cell_height) = cell_pixels.unwrap_or((0, 0));
  let mut header = format!(
    "{CACHE_MAGIC}\nwidth={width}\nheight={height}\ncell_width={cell_width}\ncell_height={cell_height}\nmode={}\ncompression=zstd\nuncompressed_bytes={plain_len}\n",
    mode.label()
  );
  if let Some(image_id) = image_id {
    header.push_str(&format!("image_id={image_id}\n"));
  }
  header.push('\n');
  let mut out = Vec::with_capacity(header.len() + compressed.len());
  out.extend_from_slice(header.as_bytes());
  out.extend_from_slice(&compressed);
  Ok(out)
}

async fn decode_cache_file(
  bytes: &[u8],
  expected_width: u16,
  expected_height: u16,
  expected_cell_pixels: Option<(u16, u16)>,
  expected_mode: RenderMode,
  expected_image_id: Option<u32>,
) -> Result<DecodedCacheFile, String> {
  let header_end = bytes
    .windows(2)
    .position(|window| window == b"\n\n")
    .ok_or_else(|| "cache metadata header missing".to_string())?;
  let header = std::str::from_utf8(&bytes[..header_end])
    .map_err(|err| format!("cache metadata is not utf-8: {err}"))?;
  let mut lines = header.lines();
  let magic = lines
    .next()
    .ok_or_else(|| "cache metadata magic missing".to_string())?;
  if magic != CACHE_MAGIC && magic != LEGACY_RAW_CACHE_MAGIC {
    return Err("cache metadata magic mismatch".to_string());
  }

  let mut width = None;
  let mut height = None;
  let mut cell_width = None;
  let mut cell_height = None;
  let mut mode = None;
  let mut compression = None;
  let mut uncompressed_bytes = None;
  let mut image_id = None;
  for line in lines {
    if let Some(value) = line.strip_prefix("width=") {
      width = value.parse::<u16>().ok();
    } else if let Some(value) = line.strip_prefix("height=") {
      height = value.parse::<u16>().ok();
    } else if let Some(value) = line.strip_prefix("cell_width=") {
      cell_width = value.parse::<u16>().ok();
    } else if let Some(value) = line.strip_prefix("cell_height=") {
      cell_height = value.parse::<u16>().ok();
    } else if let Some(value) = line.strip_prefix("mode=") {
      mode = Some(value);
    } else if let Some(value) = line.strip_prefix("compression=") {
      compression = Some(value);
    } else if let Some(value) = line.strip_prefix("uncompressed_bytes=") {
      uncompressed_bytes = value.parse::<usize>().ok();
    } else if let Some(value) = line.strip_prefix("image_id=") {
      image_id = value.parse::<u32>().ok();
    }
  }

  if width != Some(expected_width) || height != Some(expected_height) {
    return Err(format!(
      "cache size mismatch: got {:?}x{:?}, expected {}x{}",
      width, height, expected_width, expected_height
    ));
  }
  if mode != Some(expected_mode.label()) {
    return Err(format!(
      "cache mode mismatch: got {:?}, expected {}",
      mode,
      expected_mode.label()
    ));
  }
  let (expected_cell_width, expected_cell_height) = expected_cell_pixels.unwrap_or((0, 0));
  if cell_width != Some(expected_cell_width) || cell_height != Some(expected_cell_height) {
    return Err(format!(
      "cache cell size mismatch: got {:?}x{:?}, expected {}x{}",
      cell_width, cell_height, expected_cell_width, expected_cell_height
    ));
  }
  if image_id != expected_image_id {
    return Err(format!(
      "cache image id mismatch: got {:?}, expected {:?}",
      image_id, expected_image_id
    ));
  }

  let payload = &bytes[header_end + 2..];
  match compression.unwrap_or("none") {
    "none" => Ok(DecodedCacheFile {
      payload: payload.to_vec(),
      image_id,
      should_rewrite: magic != CACHE_MAGIC,
    }),
    "zstd" => {
      let expected_len = uncompressed_bytes;
      let payload = payload.to_vec();
      let decoded = tokio::task::spawn_blocking(move || decompress_zstd(payload))
        .await
        .map_err(|err| format!("zstd decompression worker failed: {err}"))?
        .map_err(|err| format!("zstd decompression failed: {err}"))?;
      if let Some(expected_len) = expected_len
        && decoded.len() != expected_len
      {
        return Err(format!(
          "cache decompressed size mismatch: got {}, expected {}",
          decoded.len(),
          expected_len
        ));
      }
      Ok(DecodedCacheFile {
        payload: decoded,
        image_id,
        should_rewrite: false,
      })
    }
    value => Err(format!("unsupported cache compression: {value}")),
  }
}

fn compress_zstd(payload: Vec<u8>, level: i32, threads: u32) -> std::io::Result<Vec<u8>> {
  let mut encoder = zstd::stream::Encoder::new(Vec::new(), level)?;
  if threads > 0 {
    encoder.multithread(threads)?;
  }
  encoder.write_all(&payload)?;
  encoder.finish()
}

fn decompress_zstd(payload: Vec<u8>) -> std::io::Result<Vec<u8>> {
  zstd::stream::decode_all(Cursor::new(payload))
}

fn hash_render_config(hasher: &mut Sha256, config: &RenderConfig) {
  hasher.update(b"render-v4");
  hasher.update([0]);
  hasher.update(config.chafa_bin.as_bytes());
  hasher.update([0]);
  hasher.update(config.chafa_threads.to_le_bytes());
  if let Some(passthrough) = &config.passthrough {
    hasher.update(passthrough.as_bytes());
  }
  hasher.update([0]);
}

fn kitty_image_id(path: &Path, width: u16, height: u16, mode: RenderMode) -> Option<u32> {
  if mode != RenderMode::Kitty {
    return None;
  }
  let mut hasher = Sha256::new();
  hasher.update(path.to_string_lossy().as_bytes());
  hasher.update(width.to_le_bytes());
  hasher.update(height.to_le_bytes());
  hasher.update(mode.label().as_bytes());
  let digest = hasher.finalize();
  let image_id = u32::from_le_bytes(digest[..4].try_into().unwrap_or_default());
  Some(image_id.max(1))
}

fn hash_native_config(hasher: &mut Sha256, config: &NativeImageConfig) {
  hasher.update(config.cell_pixels.unwrap_or((0, 0)).0.to_le_bytes());
  hasher.update(config.cell_pixels.unwrap_or((0, 0)).1.to_le_bytes());
  hasher.update([0]);
  if let Some(passthrough) = &config.passthrough {
    hasher.update(passthrough.as_bytes());
  }
  hasher.update([0]);
}

fn render_fingerprint(bytes: &[u8]) -> u64 {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  let digest = hasher.finalize();
  u64::from_le_bytes(digest[..8].try_into().unwrap_or_default())
}
