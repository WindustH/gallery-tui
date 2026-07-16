use std::{
  collections::{HashMap, HashSet, VecDeque},
  io::{Cursor, Write as IoWrite},
  path::{Path, PathBuf},
  process::Command,
  sync::{Arc, Mutex},
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
  config::RenderConfig,
  event::{AsyncEvent, RenderOutcome, RenderedImage},
  model::ImageItem,
};
use img_tui::{
  NativeImageConfig, ProtocolPlacement, RenderMode,
  native_image::{self},
};

pub struct RenderStore {
  cache_dir: PathBuf,
  config: RenderConfig,
  native_config: NativeImageConfig,
  modes: Vec<RenderMode>,
  memory: MemoryCache<RenderedImage>,
  compressed_memory: CompressedMemoryCache,
  failures: HashMap<String, String>,
  in_flight: HashSet<String>,
  semaphore: Arc<Semaphore>,
  preload_semaphore: Arc<Semaphore>,
}

type CompressedMemoryCache = Arc<Mutex<MemoryCache<Vec<u8>>>>;

struct RenderPermits {
  _global: OwnedSemaphorePermit,
  _preload: Option<OwnedSemaphorePermit>,
}

struct MemoryCache<V> {
  max_bytes: u64,
  bytes: u64,
  entries: HashMap<String, MemoryCacheEntry<V>>,
  order: VecDeque<String>,
}

struct MemoryCacheEntry<V> {
  value: V,
  size: u64,
}

impl<V> MemoryCache<V> {
  fn new(max_bytes: u64) -> Self {
    Self {
      max_bytes,
      bytes: 0,
      entries: HashMap::new(),
      order: VecDeque::new(),
    }
  }

  fn insert(&mut self, key: String, value: V, size: u64) {
    if let Some(old) = self.entries.remove(&key) {
      self.bytes = self.bytes.saturating_sub(old.size);
      self.remove_from_order(&key);
    }

    self.bytes = self.bytes.saturating_add(size);
    self
      .entries
      .insert(key.clone(), MemoryCacheEntry { value, size });
    self.order.push_back(key);
    self.enforce_limit();
  }

  fn remove(&mut self, key: &str) {
    if let Some(old) = self.entries.remove(key) {
      self.bytes = self.bytes.saturating_sub(old.size);
      self.remove_from_order(key);
    }
  }

  fn clear(&mut self) {
    self.bytes = 0;
    self.entries.clear();
    self.order.clear();
  }

  fn contains_key(&mut self, key: &str) -> bool {
    if !self.entries.contains_key(key) {
      return false;
    }
    self.remove_from_order(key);
    self.order.push_back(key.to_string());
    true
  }

  fn remove_from_order(&mut self, key: &str) {
    if let Some(index) = self.order.iter().position(|entry| entry == key) {
      self.order.remove(index);
    }
  }

  fn enforce_limit(&mut self) {
    if self.max_bytes == 0 {
      return;
    }

    while self.bytes > self.max_bytes && self.entries.len() > 1 {
      let Some(key) = self.order.pop_front() else {
        break;
      };
      if let Some(old) = self.entries.remove(&key) {
        self.bytes = self.bytes.saturating_sub(old.size);
      }
    }
  }
}

impl<V: Clone> MemoryCache<V> {
  fn get(&mut self, key: &str) -> Option<V> {
    let value = self.entries.get(key)?.value.clone();
    self.remove_from_order(key);
    self.order.push_back(key.to_string());
    Some(value)
  }
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
    let raw_memory_cache_max_bytes = config.raw_memory_cache_max_bytes;
    let compressed_memory_cache_max_bytes = config.compressed_memory_cache_max_bytes;
    Self {
      cache_dir,
      config,
      native_config,
      modes,
      memory: MemoryCache::new(raw_memory_cache_max_bytes),
      compressed_memory: Arc::new(Mutex::new(MemoryCache::new(
        compressed_memory_cache_max_bytes,
      ))),
      failures: HashMap::new(),
      in_flight: HashSet::new(),
      semaphore: Arc::new(Semaphore::new(max_concurrent)),
      preload_semaphore: Arc::new(Semaphore::new(max_preloads)),
    }
  }

  pub fn get(&mut self, item: &ImageItem, width: u16, height: u16) -> Option<RenderedImage> {
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
    let compressed_memory = self.compressed_memory.clone();
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
        compressed_memory,
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

  pub fn draws_with_protocol(&self) -> bool {
    self.modes.iter().any(|mode| mode.is_protocol())
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
        let size = rendered_image_size(&text);
        self.memory.insert(outcome.cache_key, text, size);
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

  pub fn clear_memory_caches(&mut self) {
    self.memory.clear();
    if let Ok(mut compressed_memory) = self.compressed_memory.lock() {
      compressed_memory.clear();
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
  compressed_memory: CompressedMemoryCache,
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
    let compressed_cache_key =
      render_cache_key(&image_path, width, height, &config, &native_config, mode);
    let cache_path = cache_dir.join(format!("{compressed_cache_key}.ansi"));
    let rendered = if mode.is_protocol() {
      render_or_read_cache(
        image_path.clone(),
        cache_path,
        compressed_cache_key,
        width,
        height,
        config.clone(),
        native_config.clone(),
        mode,
        compressed_memory.clone(),
        Some(&mut prepared_native),
      )
      .await
    } else {
      render_or_read_cache(
        image_path.clone(),
        cache_path,
        compressed_cache_key,
        width,
        height,
        config.clone(),
        native_config.clone(),
        mode,
        compressed_memory.clone(),
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
  compressed_cache_key: String,
  width: u16,
  height: u16,
  config: RenderConfig,
  native_config: NativeImageConfig,
  mode: RenderMode,
  compressed_memory: CompressedMemoryCache,
  prepared_native: Option<&mut Option<Result<native_image::PreparedNativeImage, String>>>,
) -> Result<RenderedImage, String> {
  let image_id = kitty_image_id(&image_path, width, height, mode);
  let placement_id = kitty_placement_id(mode, image_id);

  if let Some(bytes) = compressed_cache_get(&compressed_memory, &compressed_cache_key) {
    match decode_cache_file(
      &bytes,
      width,
      height,
      native_config.cell_pixels,
      mode,
      image_id,
      placement_id,
    )
    .await
    {
      Ok(decoded) => {
        debug!(
          path = %image_path.display(),
          mode = mode.label(),
          cache_tier = "compressed-memory",
          "render cache hit"
        );
        return decode_rendered(
          decoded.payload,
          mode,
          &native_config,
          decoded.image_id,
          decoded.placement_id,
        );
      }
      Err(error) => {
        debug!(cache = %cache_path.display(), error, "ignoring stale compressed memory render cache");
        compressed_cache_remove(&compressed_memory, &compressed_cache_key);
      }
    }
  }

  if let Ok(bytes) = fs::read(&cache_path).await {
    match decode_cache_file(
      &bytes,
      width,
      height,
      native_config.cell_pixels,
      mode,
      image_id,
      placement_id,
    )
    .await
    {
      Ok(decoded) => {
        debug!(
          path = %image_path.display(),
          mode = mode.label(),
          cache_tier = "disk",
          "render cache hit"
        );
        if decoded.should_rewrite {
          match encode_cache_file(
            &decoded.payload,
            width,
            height,
            native_config.cell_pixels,
            mode,
            decoded.image_id,
            decoded.placement_id,
            &config,
          )
          .await
          {
            Ok(cached) => {
              compressed_cache_insert(
                &compressed_memory,
                compressed_cache_key.clone(),
                cached.clone(),
              );
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
        } else {
          compressed_cache_insert(&compressed_memory, compressed_cache_key.clone(), bytes);
        }
        cache::touch_render_cache_entry(&cache_path).await;
        return decode_rendered(
          decoded.payload,
          mode,
          &native_config,
          decoded.image_id,
          decoded.placement_id,
        );
      }
      Err(error) => {
        debug!(cache = %cache_path.display(), error, "ignoring stale render cache");
      }
    }
  }

  debug!(
    path = %image_path.display(),
    mode = mode.label(),
    cache_tier = "compute",
    "render cache miss"
  );

  let can_write_disk_cache = if let Some(parent) = cache_path.parent() {
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
  } else {
    true
  };

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
          Ok(prepared) => {
            render_prepared_protocol(
              prepared,
              mode,
              &native_config,
              image_id,
              placement_id,
              width,
              height,
            )
            .await?
          }
          Err(error) => return Err(error.clone()),
        }
      }
      None => {
        let prepared = native_image::prepare(&image_path, width, height, native_config.cell_pixels)
          .await
          .map_err(|err| err.to_string())?;
        render_prepared_protocol(
          &prepared,
          mode,
          &native_config,
          image_id,
          placement_id,
          width,
          height,
        )
        .await?
      }
    }
  } else {
    RenderedBytes {
      data: run_chafa(&image_path, width, height, &config, mode).await?,
      refresh: None,
    }
  };

  match encode_cache_file(
    &bytes,
    width,
    height,
    native_config.cell_pixels,
    mode,
    image_id,
    placement_id,
    &config,
  )
  .await
  {
    Ok(cached) => {
      compressed_cache_insert(&compressed_memory, compressed_cache_key, cached.clone());
      if can_write_disk_cache {
        match fs::write(&cache_path, cached).await {
          Ok(()) => cache::touch_render_cache_entry(&cache_path).await,
          Err(error) => {
            warn!(
              cache = %cache_path.display(),
              %error,
              "failed to write render cache"
            );
          }
        }
      }
    }
    Err(error) => {
      warn!(
        cache = %cache_path.display(),
        %error,
        "failed to encode render cache"
      );
    }
  }
  decode_rendered(bytes, mode, &native_config, image_id, placement_id)
}

async fn render_prepared_protocol(
  prepared: &native_image::PreparedNativeImage,
  mode: RenderMode,
  native_config: &NativeImageConfig,
  image_id: Option<u32>,
  placement_id: Option<u32>,
  width: u16,
  height: u16,
) -> Result<RenderedBytes, String> {
  if mode == RenderMode::Kitty
    && let Some(placement_id) = placement_id
  {
    let viewport = native_image::NativeImageViewport {
      full_width_cells: width,
      full_height_cells: height,
      visible_width_cells: width,
      visible_height_cells: height,
      left_cells: 0,
      top_cells: 0,
    };
    let image_id = image_id.unwrap_or(1);
    let upload = native_image::render_prepared_kitty_upload(prepared, native_config, image_id)
      .await
      .map_err(|err| err.to_string())?;
    let refresh = native_image::render_kitty_viewport_from_upload(
      &upload,
      viewport,
      native_config,
      placement_id,
    )
    .map_err(|err| err.to_string())?;
    Ok(RenderedBytes {
      data: upload.data,
      refresh: Some(refresh),
    })
  } else {
    native_image::render_prepared(prepared, mode, native_config, image_id)
      .await
      .map(|data| RenderedBytes {
        data,
        refresh: None,
      })
      .map_err(|err| err.to_string())
  }
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
  bytes: RenderedBytes,
  mode: RenderMode,
  native_config: &NativeImageConfig,
  image_id: Option<u32>,
  placement_id: Option<u32>,
) -> Result<RenderedImage, String> {
  if mode.is_protocol() {
    let fingerprint = render_fingerprint(&bytes.data);
    let data = String::from_utf8(bytes.data).map_err(|err| err.to_string())?;
    let refresh = bytes
      .refresh
      .map(String::from_utf8)
      .transpose()
      .map_err(|err| err.to_string())?;
    let placement = match (
      mode,
      native_config.kitty_unicode_placeholders,
      image_id,
      placement_id,
    ) {
      (RenderMode::Kitty, _, Some(image_id), Some(placement_id)) => {
        Some(ProtocolPlacement::KittyPlacement {
          image_id,
          placement_id,
        })
      }
      (RenderMode::Kitty, true, Some(image_id), None) => {
        Some(ProtocolPlacement::KittyUnicode { image_id })
      }
      _ => None,
    };
    let erase = if mode == RenderMode::Kitty
      && let (Some(image_id), Some(placement_id)) = (image_id, placement_id)
    {
      native_image::erase_kitty_placement_sequence(
        native_config.passthrough.as_deref(),
        image_id,
        placement_id,
      )
    } else {
      native_image::erase_sequence(mode, native_config.passthrough.as_deref(), image_id)
    };
    Ok(RenderedImage::Protocol {
      mode,
      data,
      refresh,
      placement,
      fingerprint,
      erase,
    })
  } else {
    let text = bytes.data.into_text().map_err(|err| err.to_string())?;
    Ok(RenderedImage::Symbols { mode, text })
  }
}

fn rendered_image_size(image: &RenderedImage) -> u64 {
  match image {
    RenderedImage::Symbols { text, .. } => text
      .lines
      .iter()
      .flat_map(|line| line.spans.iter())
      .map(|span| span.content.len() as u64)
      .sum(),
    RenderedImage::Protocol {
      data,
      refresh,
      erase,
      ..
    } => {
      data.len() as u64
        + refresh.as_ref().map_or(0, |value| value.len() as u64)
        + erase.as_ref().map_or(0, |value| value.len() as u64)
    }
  }
}

fn compressed_cache_get(cache: &CompressedMemoryCache, key: &str) -> Option<Vec<u8>> {
  cache.lock().ok()?.get(key)
}

fn compressed_cache_insert(cache: &CompressedMemoryCache, key: String, bytes: Vec<u8>) {
  let size = bytes.len() as u64;
  if let Ok(mut cache) = cache.lock() {
    cache.insert(key, bytes, size);
  }
}

fn compressed_cache_remove(cache: &CompressedMemoryCache, key: &str) {
  if let Ok(mut cache) = cache.lock() {
    cache.remove(key);
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

const CACHE_MAGIC: &str = "gallery-tui-cache-v7";
const LEGACY_RAW_CACHE_MAGIC: &str = "gallery-tui-cache-v4";
const FRAMED_PAYLOAD_MAGIC: &[u8] = b"gallery-tui-rendered-bytes-v1\0";

#[derive(Debug, Clone)]
struct RenderedBytes {
  data: Vec<u8>,
  refresh: Option<Vec<u8>>,
}

struct DecodedCacheFile {
  payload: RenderedBytes,
  image_id: Option<u32>,
  placement_id: Option<u32>,
  should_rewrite: bool,
}

async fn encode_cache_file(
  payload: &RenderedBytes,
  width: u16,
  height: u16,
  cell_pixels: Option<(u16, u16)>,
  mode: RenderMode,
  image_id: Option<u32>,
  placement_id: Option<u32>,
  config: &RenderConfig,
) -> Result<Vec<u8>, String> {
  let compression_level = config.cache_compression_level;
  let compression_threads = config.cache_compression_threads;
  let payload_format = if payload.refresh.is_some() {
    "framed"
  } else {
    "raw"
  };
  let payload = encode_rendered_bytes(payload)?;
  let plain_len = payload.len();
  let compressed = tokio::task::spawn_blocking(move || {
    compress_zstd(payload, compression_level, compression_threads)
  })
  .await
  .map_err(|err| format!("zstd compression worker failed: {err}"))?
  .map_err(|err| format!("zstd compression failed: {err}"))?;

  let (cell_width, cell_height) = cell_pixels.unwrap_or((0, 0));
  let mut header = format!(
    "{CACHE_MAGIC}\nwidth={width}\nheight={height}\ncell_width={cell_width}\ncell_height={cell_height}\nmode={}\ncompression=zstd\npayload_format={payload_format}\nuncompressed_bytes={plain_len}\n",
    mode.label()
  );
  if let Some(image_id) = image_id {
    header.push_str(&format!("image_id={image_id}\n"));
  }
  if let Some(placement_id) = placement_id {
    header.push_str(&format!("placement_id={placement_id}\n"));
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
  expected_placement_id: Option<u32>,
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
  let mut payload_format = None;
  let mut uncompressed_bytes = None;
  let mut image_id = None;
  let mut placement_id = None;
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
    } else if let Some(value) = line.strip_prefix("payload_format=") {
      payload_format = Some(value);
    } else if let Some(value) = line.strip_prefix("uncompressed_bytes=") {
      uncompressed_bytes = value.parse::<usize>().ok();
    } else if let Some(value) = line.strip_prefix("image_id=") {
      image_id = value.parse::<u32>().ok();
    } else if let Some(value) = line.strip_prefix("placement_id=") {
      placement_id = value.parse::<u32>().ok();
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
  if placement_id != expected_placement_id {
    return Err(format!(
      "cache placement id mismatch: got {:?}, expected {:?}",
      placement_id, expected_placement_id
    ));
  }

  let payload = &bytes[header_end + 2..];
  let payload_format = payload_format.unwrap_or("raw");
  match compression.unwrap_or("none") {
    "none" => Ok(DecodedCacheFile {
      payload: decode_rendered_bytes(payload.to_vec(), payload_format)?,
      image_id,
      placement_id,
      should_rewrite: magic != CACHE_MAGIC || payload_format != "raw",
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
        payload: decode_rendered_bytes(decoded, payload_format)?,
        image_id,
        placement_id,
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

fn encode_rendered_bytes(payload: &RenderedBytes) -> Result<Vec<u8>, String> {
  let Some(refresh) = &payload.refresh else {
    return Ok(payload.data.clone());
  };
  let data_len = u64::try_from(payload.data.len())
    .map_err(|_| "render payload is too large to cache".to_string())?;
  let refresh_len = u64::try_from(refresh.len())
    .map_err(|_| "refresh payload is too large to cache".to_string())?;
  let mut out = Vec::with_capacity(
    FRAMED_PAYLOAD_MAGIC.len() + 16 + payload.data.len().saturating_add(refresh.len()),
  );
  out.extend_from_slice(FRAMED_PAYLOAD_MAGIC);
  out.extend_from_slice(&data_len.to_le_bytes());
  out.extend_from_slice(&refresh_len.to_le_bytes());
  out.extend_from_slice(&payload.data);
  out.extend_from_slice(refresh);
  Ok(out)
}

fn decode_rendered_bytes(bytes: Vec<u8>, payload_format: &str) -> Result<RenderedBytes, String> {
  if payload_format != "framed" {
    return Ok(RenderedBytes {
      data: bytes,
      refresh: None,
    });
  }
  let header_len = FRAMED_PAYLOAD_MAGIC.len() + 16;
  if bytes.len() < header_len || !bytes.starts_with(FRAMED_PAYLOAD_MAGIC) {
    return Err("framed render payload magic mismatch".to_string());
  }
  let lengths = &bytes[FRAMED_PAYLOAD_MAGIC.len()..header_len];
  let data_len = u64::from_le_bytes(
    lengths[0..8]
      .try_into()
      .map_err(|_| "render payload data length missing".to_string())?,
  );
  let refresh_len = u64::from_le_bytes(
    lengths[8..16]
      .try_into()
      .map_err(|_| "render payload refresh length missing".to_string())?,
  );
  let data_len =
    usize::try_from(data_len).map_err(|_| "render payload data length is too large".to_string())?;
  let refresh_len = usize::try_from(refresh_len)
    .map_err(|_| "render payload refresh length is too large".to_string())?;
  let data_start = header_len;
  let refresh_start = data_start.saturating_add(data_len);
  let end = refresh_start.saturating_add(refresh_len);
  if end != bytes.len() {
    return Err(format!(
      "framed render payload size mismatch: got {}, expected {}",
      bytes.len(),
      end
    ));
  }
  Ok(RenderedBytes {
    data: bytes[data_start..refresh_start].to_vec(),
    refresh: Some(bytes[refresh_start..end].to_vec()),
  })
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
  let image_id = u32::from_le_bytes(digest[..4].try_into().unwrap_or_default()) & 0x00ff_ffff;
  Some(image_id.max(1))
}

fn kitty_placement_id(mode: RenderMode, image_id: Option<u32>) -> Option<u32> {
  if mode == RenderMode::Kitty {
    image_id
  } else {
    None
  }
}

fn hash_native_config(hasher: &mut Sha256, config: &NativeImageConfig) {
  hasher.update(config.cell_pixels.unwrap_or((0, 0)).0.to_le_bytes());
  hasher.update(config.cell_pixels.unwrap_or((0, 0)).1.to_le_bytes());
  hasher.update([0]);
  if let Some(passthrough) = &config.passthrough {
    hasher.update(passthrough.as_bytes());
  }
  hasher.update([0]);
  hasher.update([u8::from(config.kitty_unicode_placeholders)]);
  hasher.update([0]);
}

fn render_fingerprint(bytes: &[u8]) -> u64 {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  let digest = hasher.finalize();
  u64::from_le_bytes(digest[..8].try_into().unwrap_or_default())
}
