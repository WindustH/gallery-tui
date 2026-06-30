use super::*;
use crate::config::{AppConfig, KeymapConfig, Settings, ThemeConfig};
use crate::event::AsyncEvent;
use crate::layout::{BrowserLayout, CanvasRect};
use crate::metadata::MetadataEdit;
use crate::model::{ImageItem, ImageMetadataEntry, SortDirection, SortField};
use crossterm::event::{
  Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use std::{path::PathBuf, time::SystemTime};
use tokio::sync::mpsc;

mod commands;
mod metadata;
mod navigation;
mod prompt;

fn test_app() -> App {
  App::new(
    PathBuf::from("/tmp/gallery-tui-test-root"),
    Settings {
      config: AppConfig::default(),
      keymap: KeymapConfig::default(),
      theme: ThemeConfig::default(),
      config_path: PathBuf::from("/tmp/gallery-tui-test-config.toml"),
      cache_dir: PathBuf::from("/tmp/gallery-tui-test-cache"),
    },
    Vec::new(),
  )
}

fn key(ch: char) -> Event {
  Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
}

fn click(column: u16, row: u16) -> Event {
  Event::Mouse(MouseEvent {
    kind: MouseEventKind::Down(MouseButton::Left),
    column,
    row,
    modifiers: KeyModifiers::NONE,
  })
}

fn wheel(kind: MouseEventKind) -> Event {
  Event::Mouse(MouseEvent {
    kind,
    column: 0,
    row: 0,
    modifiers: KeyModifiers::NONE,
  })
}

fn prompt_key(code: KeyCode) -> KeyEvent {
  KeyEvent::new(code, KeyModifiers::NONE)
}

fn prompt_key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
  KeyEvent::new(code, modifiers)
}

fn command_input(app: &App) -> &str {
  match app.prompt.as_ref().unwrap() {
    Prompt::Command { buffer } => &buffer.input,
    Prompt::Rename { .. } => panic!("expected command prompt"),
  }
}

fn rename_buffer(app: &App) -> &PromptBuffer {
  match app.prompt.as_ref().unwrap() {
    Prompt::Rename { buffer } => buffer,
    Prompt::Command { .. } => panic!("expected rename prompt"),
  }
}

fn unique_config_path(name: &str) -> PathBuf {
  unique_temp_path(name).with_extension("toml")
}

fn unique_temp_dir(name: &str) -> PathBuf {
  unique_temp_path(name)
}

fn unique_temp_path(name: &str) -> PathBuf {
  let nanos = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  std::env::temp_dir().join(format!("gallery-tui-{name}-{}-{nanos}", std::process::id()))
}

fn image(name: &str) -> ImageItem {
  ImageItem {
    path: PathBuf::from(format!("/tmp/gallery-tui-test-root/{name}")),
    file_name: name.to_string(),
    extension: "png".to_string(),
    size_bytes: 1,
    modified: Some(SystemTime::UNIX_EPOCH),
    created: Some(SystemTime::UNIX_EPOCH),
    dimensions: Some((16, 16)),
    metadata: Vec::new(),
  }
}
