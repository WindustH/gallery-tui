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
mod svg;
mod terminal;
mod ui;

use std::{
  env,
  io::{self, Write},
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use framework_tui::{EditorOptions, InputReader, edit_text_outside_tui, watch_termination_signals};
use tokio::sync::mpsc;

use crate::{
  app::{App, EditorRequest, InputEffect},
  event::AsyncEvent,
  model::{ImageItem, sort_images},
  render::RenderStore,
  terminal::Tui,
};
use img_tui::{NativeImageConfig, RenderMode, TerminalCapability, capability};

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
  spawn_cache_cleanup(
    settings.cache_dir.clone(),
    settings.config.render.disk_cache_max_bytes,
  );

  let terminal_capability = capability::detect();
  tracing::info!(?terminal_capability, "detected terminal capability");

  let mut images = scanner::scan_images(startup.root.clone(), &settings.config).await?;
  let initial_sort = settings.config.initial_sort_spec();
  sort_images(&mut images, &initial_sort);
  let focused = focus_index(&images, startup.focus.as_deref())?;

  let (tx, mut rx) = mpsc::unbounded_channel::<AsyncEvent>();
  let input_tx = tx.clone();
  let input = InputReader::spawn(move |input| {
    input_tx
      .send(AsyncEvent::Input {
        event: input.event,
        generation: input.generation,
      })
      .is_ok()
  })?;
  // Quit cleanly (restoring the terminal) on SIGTERM or SIGHUP.
  let signal_tx = tx.clone();
  watch_termination_signals(move |_| {
    let _ = signal_tx.send(AsyncEvent::Terminate);
  })?;

  let mut renderer = render_store(&settings, &terminal_capability);
  let mut app = App::new(startup.root, settings, images);
  app.focused = focused;
  if startup.focus.is_some() {
    app.enter_detail(startup.detail_back_quits);
  }
  app.terminal_cell_pixels = terminal_capability.cell_pixels;

  let mut tui = Tui::new()?;
  // Redraw only after something happened: every state change, render result,
  // and resize arrives as an event.
  loop {
    tui.draw(|frame| ui::draw(frame, &mut app, &mut renderer, &tx))?;
    if app.should_quit() {
      break;
    }
    if let Some(request) = app.take_editor_request() {
      run_editor(&mut tui, &mut app, request, &input, &temp_dir)?;
      continue;
    }
    let Some(message) = rx.recv().await else {
      break;
    };
    let effect = handle_async_event(message, &mut app, &mut renderer, &tx, &input);
    let frame_sync_navigation = app.settings.config.behavior.frame_sync_navigation;
    drain_queued_events(
      &mut rx,
      &mut app,
      &mut renderer,
      &tx,
      &input,
      frame_sync_navigation && effect == InputEffect::BrowseStep,
      frame_sync_navigation,
    );
  }

  tui.restore()?;
  if let Some(paths) = app.take_stdout_paths() {
    write_paths(&mut io::stdout().lock(), &paths)?;
  }

  Ok(())
}

/// Trim the disk render cache to `max_bytes` in the background, so a large
/// cache does not delay the first frame.
fn spawn_cache_cleanup(cache_dir: PathBuf, max_bytes: u64) {
  tokio::spawn(async move {
    match cache::enforce_render_cache_limit(&cache_dir, max_bytes).await {
      Ok(report) => tracing::info!(
        before_bytes = report.before_bytes,
        after_bytes = report.after_bytes,
        removed_files = report.removed_files,
        removed_bytes = report.removed_bytes,
        max_bytes,
        "render cache cleanup finished"
      ),
      Err(error) => tracing::warn!(%error, "render cache cleanup failed"),
    }
  });
}

/// Environment variable that forces the render modes, e.g. `sixel,symbols`.
const RENDER_MODES_ENV: &str = "GALLERY_TUI_RENDER_MODES";

/// Pick the render mode order and build the render store.
fn render_store(settings: &config::Settings, capability: &TerminalCapability) -> RenderStore {
  let mut render = settings.config.render.clone();
  if render.auto_detect {
    render.apply_terminal_capability(capability);
    tracing::info!(?render.chafa_args, "selected chafa fallback mode");
  }
  let modes = if let Some(modes) = capability::render_modes_override_from_env(RENDER_MODES_ENV) {
    tracing::info!(
      env = RENDER_MODES_ENV,
      modes = ?modes.iter().map(|mode| mode.label()).collect::<Vec<_>>(),
      "render mode order overridden by environment"
    );
    modes
  } else if render.auto_detect {
    capability.preferred_render_modes(&render.zellij_sixel)
  } else {
    vec![RenderMode::Symbols, RenderMode::Ascii]
  };
  tracing::info!(
    modes = ?modes.iter().map(|mode| mode.label()).collect::<Vec<_>>(),
    "render mode order"
  );
  let native_config = NativeImageConfig {
    cell_pixels: capability.cell_pixels,
    passthrough: capability.passthrough().map(str::to_string),
    kitty_unicode_placeholders: capability.kitty_unicode_placeholders(),
  };
  RenderStore::new(settings.cache_dir.clone(), render, native_config, modes)
}

/// Hand the terminal to `$EDITOR`, then apply the edited text.
fn run_editor(
  tui: &mut Tui,
  app: &mut App,
  request: EditorRequest,
  input: &InputReader,
  temp_dir: &Path,
) -> Result<()> {
  let handoff = edit_text_outside_tui(
    tui,
    Some(input),
    request.initial_text(),
    temp_dir,
    &EditorOptions::default(),
  );
  let result = handoff.output;
  match request {
    EditorRequest::Prompt { .. } => app.finish_prompt_editor_input(result),
    EditorRequest::Metadata { path, original, .. } => {
      app.finish_metadata_editor_input(path, original, result)
    }
  }
  handoff.terminal
}

/// Print one path per line. On Unix the raw bytes are written so paths that
/// are not valid UTF-8 survive a pipe unchanged.
fn write_paths(out: &mut impl Write, paths: &[PathBuf]) -> io::Result<()> {
  for path in paths {
    #[cfg(unix)]
    {
      use std::os::unix::ffi::OsStrExt;
      out.write_all(path.as_os_str().as_bytes())?;
      out.write_all(b"\n")?;
    }
    #[cfg(not(unix))]
    writeln!(out, "{}", path.display())?;
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
  input: &InputReader,
) -> InputEffect {
  match message {
    AsyncEvent::Input { event, generation } => {
      if input.is_current(generation) {
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
      renderer.forget_failures();
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
    AsyncEvent::Terminate => {
      app.request_quit();
      InputEffect::Other
    }
  }
}

fn drain_queued_events(
  rx: &mut mpsc::UnboundedReceiver<AsyncEvent>,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  input: &InputReader,
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
    if discard_browse_inputs && queued_message_is_frame_sync_deferred_input(&message, app, input) {
      continue;
    }
    let effect = handle_async_event(message, app, renderer, tx, input);
    if frame_sync_navigation && effect == InputEffect::BrowseStep {
      discard_browse_inputs = true;
    }
  }
}

fn queued_message_is_frame_sync_deferred_input(
  message: &AsyncEvent,
  app: &App,
  input: &InputReader,
) -> bool {
  match message {
    AsyncEvent::Input { event, generation } => {
      input.is_current(*generation) && app.input_deferred_by_frame_sync(event)
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

fn focus_index(images: &[ImageItem], focus: Option<&Path>) -> Result<usize> {
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
