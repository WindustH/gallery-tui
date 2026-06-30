mod app;
mod cache;
mod capability;
mod config;
mod event;
mod keymap;
mod layout;
mod logging;
mod metadata;
mod model;
mod native_image;
mod render;
mod scanner;
mod terminal;
mod ui;

use std::{
  env, fs,
  io::{self, Write},
  path::PathBuf,
  process::Command,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  thread,
  time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::event as crossterm_event;
use tokio::sync::mpsc;

use crate::{
  app::{App, EditorRequest},
  capability::RenderMode,
  event::AsyncEvent,
  model::sort_images,
  native_image::NativeImageConfig,
  render::RenderStore,
  terminal::Tui,
};

#[derive(Debug, Parser)]
#[command(
  version,
  about = "Browse image folders in a terminal UI using ratatui and chafa"
)]
struct Cli {
  /// Folder containing images.
  path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();
  let root = cli
    .path
    .canonicalize()
    .with_context(|| format!("failed to resolve {}", cli.path.display()))?;
  if !root.is_dir() {
    bail!("{} is not a directory", root.display());
  }

  let settings = config::load_or_create().await?;
  let log_path = logging::init(&settings.cache_dir)?;
  tracing::info!(cache_dir = %settings.cache_dir.display(), log_path = %log_path.display(), "gallery-tui starting");
  match cache::enforce_render_cache_limit(
    &settings.cache_dir,
    settings.config.render.cache_max_bytes,
  )
  .await
  {
    Ok(report) => tracing::info!(
      before_bytes = report.before_bytes,
      after_bytes = report.after_bytes,
      removed_files = report.removed_files,
      removed_bytes = report.removed_bytes,
      max_bytes = settings.config.render.cache_max_bytes,
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
  let render_modes = if effective_render.auto_detect {
    terminal_capability.preferred_render_modes(&effective_render.zellij_sixel)
  } else {
    vec![RenderMode::Symbols, RenderMode::Ascii]
  };
  tracing::info!(
      modes = ?render_modes.iter().map(|mode| mode.label()).collect::<Vec<_>>(),
      "render mode order"
  );

  let mut images = scanner::scan_images(root.clone(), &settings.config).await?;
  let initial_sort = settings.config.initial_sort_spec();
  sort_images(&mut images, &initial_sort);

  let (tx, mut rx) = mpsc::unbounded_channel::<AsyncEvent>();
  let input_enabled = Arc::new(AtomicBool::new(true));
  spawn_input_thread(tx.clone(), input_enabled.clone());

  let mut app = App::new(root, settings, images);
  app.terminal_cell_pixels = terminal_capability.cell_pixels;
  let native_config = NativeImageConfig {
    cell_pixels: terminal_capability.cell_pixels,
    passthrough: terminal_capability.passthrough().map(str::to_string),
  };
  let protocol_reset = render_modes
    .contains(&RenderMode::Kitty)
    .then(|| {
      native_image::erase_sequence(
        RenderMode::Kitty,
        native_config.passthrough.as_deref(),
        None,
      )
    })
    .flatten();
  let mut renderer = RenderStore::new(
    app.settings.cache_dir.clone(),
    effective_render,
    native_config,
    render_modes,
  );

  let mut tui = Tui::new(protocol_reset)?;
  loop {
    if app.confirm.is_some() {
      tui.clear_protocol_overlays()?;
    }
    tui.draw(|frame| ui::draw(frame, &mut app, &mut renderer, &tx))?;
    tui.render_protocol_overlays(&app.protocol_overlays)?;
    if app.should_quit() {
      break;
    }
    if let Some(request) = app.take_editor_request() {
      input_enabled.store(false, Ordering::SeqCst);
      tui.suspend()?;
      let result = edit_text_in_editor(request.initial_text(), &app.settings.cache_dir);
      let resume_result = tui.resume();
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
            match message {
                AsyncEvent::Input(input) => app.handle_input(input, &tx),
                AsyncEvent::Render(outcome) => {
                    if let Some(error) = renderer.finish(outcome) {
                        app.set_message(error);
                    }
                }
                AsyncEvent::Scan(outcome) => app.finish_scan(outcome),
                AsyncEvent::Rename(outcome) => app.finish_rename(outcome),
                AsyncEvent::CacheClear(outcome) => app.finish_cache_clear(outcome),
                AsyncEvent::ConfigSave(outcome) => app.finish_config_save(outcome),
                AsyncEvent::MetadataWrite(outcome) => app.finish_metadata_write(outcome),
            }
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

fn spawn_input_thread(tx: mpsc::UnboundedSender<AsyncEvent>, enabled: Arc<AtomicBool>) {
  thread::spawn(move || {
    loop {
      if !enabled.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(25));
        continue;
      }
      match crossterm_event::poll(Duration::from_millis(50)) {
        Ok(true) => {
          let Ok(input) = crossterm_event::read() else {
            break;
          };
          if tx.send(AsyncEvent::Input(input)).is_err() {
            break;
          }
        }
        Ok(false) => {}
        Err(_) => break,
      }
    }
  });
}

fn edit_text_in_editor(initial: &str, cache_dir: &std::path::Path) -> Result<String, String> {
  let editor = env::var("EDITOR")
    .or_else(|_| env::var("VISUAL"))
    .unwrap_or_else(|_| "vi".to_string());
  let editor_dir = cache_dir.join("editor");
  fs::create_dir_all(&editor_dir).map_err(|err| err.to_string())?;
  let nanos = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .map_err(|err| err.to_string())?
    .as_nanos();
  let path = editor_dir.join(format!("input-{}-{nanos}.txt", std::process::id()));
  fs::write(&path, initial).map_err(|err| err.to_string())?;

  let status = Command::new("sh")
    .arg("-c")
    .arg(format!(
      "{} {}",
      editor,
      shell_quote(&path.display().to_string())
    ))
    .status()
    .map_err(|err| err.to_string())?;
  if !status.success() {
    let _ = fs::remove_file(&path);
    return Err(format!("editor exited with {status}"));
  }
  let edited = fs::read_to_string(&path).map_err(|err| err.to_string())?;
  let _ = fs::remove_file(&path);
  Ok(edited.trim_end_matches(['\r', '\n']).to_string())
}

fn shell_quote(value: &str) -> String {
  let mut quoted = String::from("'");
  for ch in value.chars() {
    if ch == '\'' {
      quoted.push_str("'\\''");
    } else {
      quoted.push(ch);
    }
  }
  quoted.push('\'');
  quoted
}
