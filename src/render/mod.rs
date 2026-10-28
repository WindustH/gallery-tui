//! On-demand image rendering.
//!
//! `RenderStore` owns the in-memory render cache and schedules render jobs on
//! the tokio runtime. Each job (see `pipeline`) walks the configured render
//! modes and the compressed-memory and disk cache tiers; results come back to
//! the UI loop as `AsyncEvent::Render`.

mod cache_file;
mod keys;
mod pipeline;

use std::{
  collections::{HashMap, HashSet},
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
};

use img_tui::{NativeImageConfig, NativeImageViewport, ProtocolImage, RenderMode};
use ratatui::widgets::Paragraph;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

use crate::{config::RenderConfig, model::ImageItem};
use img_tui::cache::MemoryCache as LruCache;
use pipeline::RenderContext;

/// A finished render ready to be drawn.
#[derive(Debug)]
pub enum RenderedImage {
  /// Chafa text output, drawn as ordinary ratatui cells.
  Symbols {
    paragraph: Box<Paragraph<'static>>,
    size: u64,
  },
  /// A terminal graphics protocol payload, written after each frame.
  Protocol(ProtocolImage),
}

impl RenderedImage {
  fn size(&self) -> u64 {
    match self {
      Self::Symbols { size, .. } => *size,
      Self::Protocol(image) => image.payload_len() as u64,
    }
  }
}

/// Identity of one render in the in-memory cache. Everything else that
/// affects the output (render config, modes, terminal) is fixed for the
/// lifetime of a `RenderStore`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderKey {
  path: PathBuf,
  size_bytes: u64,
  modified: u128,
  width: u16,
  height: u16,
  viewport: Option<NativeImageViewport>,
}

impl RenderKey {
  fn for_viewport(item: &ImageItem, viewport: NativeImageViewport) -> Self {
    let mut key = Self::new(item, viewport.full_width_cells, viewport.full_height_cells);
    if viewport.left_cells != 0
      || viewport.top_cells != 0
      || viewport.visible_width_cells != key.width
      || viewport.visible_height_cells != key.height
    {
      key.viewport = Some(viewport);
    }
    key
  }
  fn new(item: &ImageItem, width: u16, height: u16) -> Self {
    Self {
      path: item.path.clone(),
      size_bytes: item.size_bytes,
      modified: item.modified_key(),
      width,
      height,
      viewport: None,
    }
  }
}

#[derive(Debug)]
pub struct RenderOutcome {
  preload: bool,
  key: RenderKey,
  result: Result<RenderedImage, RenderFailure>,
}

#[derive(Debug)]
enum RenderFailure {
  /// The job waited for a worker slot until nothing on screen wanted it.
  Cancelled,
  Error(String),
}

/// What the UI should draw for a requested render.
pub enum RenderState<'a> {
  Ready(&'a RenderedImage),
  Failed(&'a str),
  Pending,
}

struct RenderPermits {
  _global: OwnedSemaphorePermit,
  _preload: Option<OwnedSemaphorePermit>,
}

/// A queued job is dropped when it was not requested in either of the last
/// two frames by the time a worker slot frees up.
const STALE_AFTER_FRAMES: u64 = 2;

pub struct RenderStore {
  context: Arc<RenderContext>,
  memory: LruCache<RenderKey, RenderedImage>,
  current: Option<Arc<RenderedImage>>,
  failures: HashMap<RenderKey, String>,
  preload_failures: HashMap<RenderKey, std::time::Instant>,
  visible_waits: HashSet<RenderKey>,
  /// Jobs in progress, with the last frame that asked for each.
  in_flight: HashMap<RenderKey, Arc<AtomicU64>>,
  frame: Arc<AtomicU64>,
  max_concurrent: usize,
  semaphore: Arc<Semaphore>,
  preload_semaphore: Arc<Semaphore>,
}

impl RenderStore {
  pub fn new(
    cache_dir: PathBuf,
    config: RenderConfig,
    native_config: NativeImageConfig,
    modes: Vec<RenderMode>,
  ) -> Self {
    let max_concurrent = config.max_concurrent.max(1);
    // Share the raw-memory budget between terminal renders and reusable decoded images.
    let raw_budget = config.raw_memory_cache_max_bytes;
    let prepared_budget = if raw_budget == 0 {
      0
    } else {
      (raw_budget / 2).max(1)
    };
    let memory = LruCache::new(if raw_budget == 0 {
      0
    } else {
      raw_budget.saturating_sub(prepared_budget).max(1)
    });
    let compressed_memory = Arc::new(LruCache::new(config.compressed_memory_cache_max_bytes));
    let cache_writer = pipeline::cache_writer(&config, compressed_memory.clone());
    Self {
      context: Arc::new(RenderContext {
        cache_dir,
        config,
        native_config,
        modes,
        compressed_memory,
        cache_writer,
        prepared: img_tui::slicing::PreparedImageCache::new(prepared_budget),
        session_nonce: keys::render_session_nonce(),
      }),
      memory,
      current: None,
      failures: HashMap::new(),
      preload_failures: HashMap::new(),
      visible_waits: HashSet::new(),
      in_flight: HashMap::new(),
      frame: Arc::new(AtomicU64::new(0)),
      max_concurrent,
      semaphore: Arc::new(Semaphore::new(max_concurrent)),
      preload_semaphore: Arc::new(Semaphore::new(max_concurrent - 1)),
    }
  }

  /// Mark the start of a UI frame. Queued jobs that no frame asks for any
  /// more are skipped instead of rendered.
  pub fn begin_frame(&mut self) {
    self.frame.fetch_add(1, Ordering::Relaxed);
  }

  /// Request a visible slice at the full image scale, starting a job on a cache miss.
  pub fn request_viewport<E: From<RenderOutcome> + Send + 'static>(
    &mut self,
    item: &ImageItem,
    viewport: NativeImageViewport,
    tx: &mpsc::UnboundedSender<E>,
  ) -> RenderState<'_> {
    let width = viewport.full_width_cells;
    let height = viewport.full_height_cells;
    if width == 0 || height == 0 {
      return RenderState::Pending;
    }
    let key = RenderKey::for_viewport(item, viewport);
    self.current = self.memory.get(&key);
    if self.current.is_some() {
      return RenderState::Ready(self.current.as_deref().unwrap());
    }
    if self.failures.contains_key(&key) {
      return self
        .failures
        .get(&key)
        .map_or(RenderState::Pending, |error| RenderState::Failed(error));
    }
    if !self.mark_in_flight(&key) {
      self.spawn(key, tx, None);
    } else {
      self.visible_waits.insert(key);
    }
    RenderState::Pending
  }

  /// Start a background render for an image near the focus, but only when a
  /// worker slot is free right now.
  pub fn preload<E: From<RenderOutcome> + Send + 'static>(
    &mut self,
    item: &ImageItem,
    width: u16,
    height: u16,
    tx: &mpsc::UnboundedSender<E>,
  ) {
    self.preload_viewport(
      item,
      NativeImageViewport {
        full_width_cells: width,
        full_height_cells: height,
        visible_width_cells: width,
        visible_height_cells: height,
        left_cells: 0,
        top_cells: 0,
      },
      tx,
    );
  }

  pub fn preload_viewport<E: From<RenderOutcome> + Send + 'static>(
    &mut self,
    item: &ImageItem,
    viewport: NativeImageViewport,
    tx: &mpsc::UnboundedSender<E>,
  ) {
    let width = viewport.visible_width_cells;
    let height = viewport.visible_height_cells;
    if width == 0 || height == 0 {
      return;
    }
    let key = RenderKey::for_viewport(item, viewport);
    self
      .preload_failures
      .retain(|_, failed| failed.elapsed().as_secs() < 5);
    if self.preload_failures.contains_key(&key) {
      return;
    }
    if self.memory.touch(&key) || self.failures.contains_key(&key) || self.mark_in_flight(&key) {
      return;
    }
    if self.in_flight.len() >= self.max_concurrent {
      return;
    }
    if let Some(permits) = self.try_preload_permits() {
      self.spawn(key, tx, Some(permits));
    }
  }

  pub fn draws_with_protocol(&self) -> bool {
    self.context.modes.iter().any(|mode| mode.is_protocol())
  }

  /// Apply a finished job. Returns a status message for failures.
  pub fn finish(&mut self, outcome: RenderOutcome) -> (bool, Option<String>) {
    let waiting = self.visible_waits.remove(&outcome.key);
    let visible = !outcome.preload || waiting;
    self.in_flight.remove(&outcome.key);
    match outcome.result {
      Ok(image) => {
        self.failures.remove(&outcome.key);
        let size = image.size();
        self.memory.insert(outcome.key, image, size);
        (visible, None)
      }
      Err(RenderFailure::Cancelled) => (visible, None),
      Err(RenderFailure::Error(_)) if outcome.preload => {
        self
          .preload_failures
          .insert(outcome.key, std::time::Instant::now());
        (visible, None)
      }
      Err(RenderFailure::Error(error)) => {
        let message = format!("render failed: {error}");
        self.failures.insert(outcome.key, error);
        (true, Some(message))
      }
    }
  }

  pub fn clear_memory_caches(&mut self) {
    self.current = None;
    self.memory.clear();
    self.context.clear_compressed_memory();
    self.context.prepared.clear();
    self.forget_failures();
  }

  /// Let failed renders be retried, e.g. after a rescan or once a missing
  /// file or tool is back.
  pub fn forget_failures(&mut self) {
    self.failures.clear();
    self.preload_failures.clear();
  }

  pub fn has_preload_capacity(&self) -> bool {
    self.in_flight.len() < self.max_concurrent
      && self.preload_semaphore.available_permits() > 0
      && self.semaphore.available_permits() > 0
  }

  fn try_preload_permits(&self) -> Option<RenderPermits> {
    let preload = self.preload_semaphore.clone().try_acquire_owned().ok()?;
    let global = self.semaphore.clone().try_acquire_owned().ok()?;
    Some(RenderPermits {
      _global: global,
      _preload: Some(preload),
    })
  }

  /// Record that the current frame wants `key`; false when no job for it is
  /// in progress.
  fn mark_in_flight(&self, key: &RenderKey) -> bool {
    let Some(requested) = self.in_flight.get(key) else {
      return false;
    };
    requested.store(self.frame.load(Ordering::Relaxed), Ordering::Relaxed);
    true
  }

  fn spawn<E: From<RenderOutcome> + Send + 'static>(
    &mut self,
    key: RenderKey,
    tx: &mpsc::UnboundedSender<E>,
    permits: Option<RenderPermits>,
  ) {
    let preload = permits.is_some();
    let requested = Arc::new(AtomicU64::new(self.frame.load(Ordering::Relaxed)));
    self.in_flight.insert(key.clone(), requested.clone());

    let context = self.context.clone();
    let current_frame = self.frame.clone();
    let semaphore = self.semaphore.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
      let job = {
        let key = key.clone();
        async move {
          let _permits = match permits {
            Some(permits) => permits,
            None => RenderPermits {
              _global: semaphore
                .acquire_owned()
                .await
                .map_err(|err| RenderFailure::Error(err.to_string()))?,
              _preload: None,
            },
          };
          let last_requested = requested.load(Ordering::Relaxed);
          if last_requested + STALE_AFTER_FRAMES < current_frame.load(Ordering::Relaxed) {
            return Err(RenderFailure::Cancelled);
          }
          context
            .render(&key.path, key.width, key.height, key.viewport)
            .await
            .map_err(RenderFailure::Error)
        }
      };
      // Run the job as its own task so a panic inside it still reports back
      // and frees the in-flight slot instead of leaving the card pending.
      let result = match tokio::spawn(job).await {
        Ok(result) => result,
        Err(error) => Err(RenderFailure::Error(format!("render task failed: {error}"))),
      };
      let _ = tx.send(E::from(RenderOutcome {
        key,
        result,
        preload,
      }));
    });
  }
}
