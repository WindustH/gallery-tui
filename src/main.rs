mod app;
mod cache;
mod config;
mod event;
mod fs_atomic;
mod layout;
mod logging;
mod metadata;
mod model;
mod render;
mod scanner;
mod terminal;
mod ui;

use std::{
  env,
  io::{self, Write},
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  thread,
  time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::event as crossterm_event;
use framework_tui::edit_text_in_editor;
use tokio::sync::mpsc;

use crate::{
  app::{App, EditorRequest, InputEffect},
  event::AsyncEvent,
  model::sort_images,
  render::RenderStore,
  terminal::Tui,
};
use img_tui::{NativeImageConfig, RenderMode, capability};

#[derive(Debug, Parser)]
#[command(
  version,
  about = "Browse image folders in a terminal UI using ratatui and chafa"
)]
struct Cli {
  /// When opening a single image, let q return from detail to browser instead of quitting.
  #[arg(long)]
  browser: bool,

  /// Image file or folder containing images.
  path: PathBuf,
}

#[derive(Debug)]
struct StartupTarget {
  root: PathBuf,
  focus: Option<PathBuf>,
  detail_back_quits: bool,
}

const MAX_QUEUED_EVENTS_PER_TICK: usize = 256;

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();
  let input = cli
    .path
    .canonicalize()
    .with_context(|| format!("failed to resolve {}", cli.path.display()))?;
  let startup = startup_target(input, cli.browser)?;

  let settings = config::load_or_create().await?;
  let log_path = logging::init(&settings.cache_dir)?;
  let temp_dir = gallery_temp_dir();
  tracing::info!(
    cache_dir = %settings.cache_dir.display(),
    temp_dir = %temp_dir.display(),
    log_path = %log_path.display(),
    "gallery-tui starting"
  );
  match cache::enforce_render_cache_limit(
    &settings.cache_dir,
    settings.config.render.disk_cache_max_bytes,
  )
  .await
  {
    Ok(report) => tracing::info!(
      before_bytes = report.before_bytes,
      after_bytes = report.after_bytes,
      removed_files = report.removed_files,
      removed_bytes = report.removed_bytes,
      max_bytes = settings.config.render.disk_cache_max_bytes,
      "render cache cleanup finished"
    ),
    Err(error) => tracing::warn!(%error, "render cache cleanup failed"),
  }

  let terminal_capability = capability::detect();
  tracing::info!(?terminal_capability, "detected terminal capability");

  let mut effective_render = settings.config.render.clone();
  if effective_render.auto_detect {
    effective_render.apply_terminal_capability(&terminal_capability);
    tracing::info!(?effective_render.chafa_args, "selected chafa fallback mode");
  }
  let render_modes = if let Some(modes) = capability::render_modes_override_from_env() {
    tracing::info!(
      env = capability::RENDER_MODES_ENV,
      modes = ?modes.iter().map(|mode| mode.label()).collect::<Vec<_>>(),
      "render mode order overridden by environment"
    );
    modes
  } else if effective_render.auto_detect {
    terminal_capability.preferred_render_modes(&effective_render.zellij_sixel)
  } else {
    vec![RenderMode::Symbols, RenderMode::Ascii]
  };
  tracing::info!(
      modes = ?render_modes.iter().map(|mode| mode.label()).collect::<Vec<_>>(),
      "render mode order"
  );

  let mut images = scanner::scan_images(startup.root.clone(), &settings.config).await?;
  let initial_sort = settings.config.initial_sort_spec();
  sort_images(&mut images, &initial_sort);
  let focused = focus_index(&images, startup.focus.as_deref())?;

  let (tx, mut rx) = mpsc::unbounded_channel::<AsyncEvent>();
  let input_enabled = Arc::new(AtomicBool::new(true));
  let input_generation = Arc::new(AtomicU64::new(0));
  spawn_input_thread(tx.clone(), input_enabled.clone(), input_generation.clone());

  let mut app = App::new(startup.root, settings, images);
  app.focused = focused;
  if startup.focus.is_some() {
    app.enter_detail(startup.detail_back_quits);
  }
  app.terminal_cell_pixels = terminal_capability.cell_pixels;
  let native_config = NativeImageConfig {
    cell_pixels: terminal_capability.cell_pixels,
    passthrough: terminal_capability.passthrough().map(str::to_string),
    kitty_unicode_placeholders: terminal_capability.kitty_unicode_placeholders(),
  };
  let mut renderer = RenderStore::new(
    app.settings.cache_dir.clone(),
    effective_render,
    native_config,
    render_modes,
  );

  let mut tui = Tui::new()?;
  loop {
    tui.draw(|frame| ui::draw(frame, &mut app, &mut renderer, &tx))?;
    if app.should_quit() {
      break;
    }
    if let Some(request) = app.take_editor_request() {
      input_enabled.store(false, Ordering::SeqCst);
      input_generation.fetch_add(1, Ordering::SeqCst);
      tui.suspend()?;
      let result = edit_text_in_editor(request.initial_text(), &temp_dir);
      let resume_result = tui.resume();
      if resume_result.is_ok() {
        discard_pending_terminal_events();
      }
      input_generation.fetch_add(1, Ordering::SeqCst);
      input_enabled.store(true, Ordering::SeqCst);
      match request {
        EditorRequest::Prompt { .. } => app.finish_prompt_editor_input(result),
        EditorRequest::Metadata { path, original, .. } => {
          app.finish_metadata_editor_input(path, original, result)
        }
      }
      resume_result?;
      continue;
    }

    tokio::select! {
      Some(message) = rx.recv() => {
        let effect = handle_async_event(message, &mut app, &mut renderer, &tx, &input_generation);
        let frame_sync_navigation = app.settings.config.behavior.frame_sync_navigation;
        drain_queued_events(
          &mut rx,
          &mut app,
          &mut renderer,
          &tx,
          &input_generation,
          frame_sync_navigation && effect == InputEffect::BrowseStep,
          frame_sync_navigation,
        );
      }
      _ = tokio::time::sleep(Duration::from_millis(33)) => {}
    }
  }

  tui.restore()?;
  if let Some(paths) = app.take_stdout_paths() {
    let mut stdout = io::stdout().lock();
    for path in paths {
      writeln!(stdout, "{}", path.display())?;
    }
  }

  Ok(())
}

fn gallery_temp_dir() -> PathBuf {
  env::var_os("GALLERY_TUI_TMPDIR")
    .filter(|value| !value.is_empty())
    .map(PathBuf::from)
    .unwrap_or_else(default_gallery_temp_dir)
}

fn default_gallery_temp_dir() -> PathBuf {
  #[cfg(unix)]
  {
    PathBuf::from("/tmp/gallery-tui")
  }
  #[cfg(not(unix))]
  {
    env::temp_dir().join("gallery-tui")
  }
}

fn handle_async_event(
  message: AsyncEvent,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  input_generation: &AtomicU64,
) -> InputEffect {
  match message {
    AsyncEvent::Input { event, generation } => {
      if generation == input_generation.load(Ordering::SeqCst) {
        app.handle_input(event, tx)
      } else {
        InputEffect::None
      }
    }
    AsyncEvent::Render(outcome) => {
      if let Some(error) = renderer.finish(outcome) {
        app.set_message(error);
      }
      InputEffect::Other
    }
    AsyncEvent::Scan(outcome) => {
      app.finish_scan(outcome);
      InputEffect::Other
    }
    AsyncEvent::Rename(outcome) => {
      app.finish_rename(outcome);
      InputEffect::Other
    }
    AsyncEvent::CacheClear(outcome) => {
      renderer.clear_memory_caches();
      app.finish_cache_clear(outcome);
      InputEffect::Other
    }
    AsyncEvent::ConfigSave(outcome) => {
      app.finish_config_save(outcome);
      InputEffect::Other
    }
    AsyncEvent::MetadataWrite(outcome) => {
      app.finish_metadata_write(outcome);
      InputEffect::Other
    }
  }
}

fn drain_queued_events(
  rx: &mut mpsc::UnboundedReceiver<AsyncEvent>,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  input_generation: &AtomicU64,
  discard_browse_inputs: bool,
  frame_sync_navigation: bool,
) {
  let mut discard_browse_inputs = discard_browse_inputs;
  for _ in 0..MAX_QUEUED_EVENTS_PER_TICK {
    if app.should_quit() || app.editor_request_pending() {
      break;
    }
    let Ok(message) = rx.try_recv() else {
      break;
    };
    if discard_browse_inputs
      && queued_message_is_frame_sync_deferred_input(&message, app, input_generation)
    {
      continue;
    }
    let effect = handle_async_event(message, app, renderer, tx, input_generation);
    if frame_sync_navigation && effect == InputEffect::BrowseStep {
      discard_browse_inputs = true;
    }
  }
}

fn queued_message_is_frame_sync_deferred_input(
  message: &AsyncEvent,
  app: &App,
  input_generation: &AtomicU64,
) -> bool {
  match message {
    AsyncEvent::Input { event, generation } => {
      *generation == input_generation.load(Ordering::SeqCst)
        && app.input_deferred_by_frame_sync(event)
    }
    _ => false,
  }
}

fn startup_target(input: PathBuf, browser: bool) -> Result<StartupTarget> {
  if input.is_dir() {
    return Ok(StartupTarget {
      root: input,
      focus: None,
      detail_back_quits: false,
    });
  }
  if input.is_file() {
    let root = input
      .parent()
      .map(Path::to_path_buf)
      .with_context(|| format!("{} has no parent directory", input.display()))?;
    return Ok(StartupTarget {
      root,
      focus: Some(input),
      detail_back_quits: !browser,
    });
  }
  bail!("{} is not a file or directory", input.display())
}

fn focus_index(images: &[crate::model::ImageItem], focus: Option<&Path>) -> Result<usize> {
  let Some(focus) = focus else {
    return Ok(0);
  };
  images
    .iter()
    .position(|item| item.path == focus)
    .with_context(|| {
      format!(
        "{} is not a supported image in this folder",
        focus.display()
      )
    })
}

fn spawn_input_thread(
  tx: mpsc::UnboundedSender<AsyncEvent>,
  enabled: Arc<AtomicBool>,
  generation: Arc<AtomicU64>,
) {
  thread::spawn(move || {
    loop {
      if !enabled.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(25));
        continue;
      }
      match crossterm_event::poll(Duration::from_millis(50)) {
        Ok(true) => {
          if !enabled.load(Ordering::SeqCst) {
            continue;
          }
          let Ok(input) = crossterm_event::read() else {
            break;
          };
          if !enabled.load(Ordering::SeqCst) {
            continue;
          }
          let generation = generation.load(Ordering::SeqCst);
          if tx
            .send(AsyncEvent::Input {
              event: input,
              generation,
            })
            .is_err()
          {
            break;
          }
        }
        Ok(false) => {}
        Err(_) => break,
      }
    }
  });
}

fn discard_pending_terminal_events() {
  for _ in 0..256 {
    match crossterm_event::poll(Duration::from_millis(0)) {
      Ok(true) => {
        if crossterm_event::read().is_err() {
          break;
        }
      }
      Ok(false) | Err(_) => break,
    }
  }
}
