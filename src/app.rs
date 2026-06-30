use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use ratatui::layout::Rect;
use tracing::debug;

use crate::{
  config::Settings,
  event::ProtocolOverlay,
  keymap::{KeyBindings, KeyHint},
  layout::BrowserLayout,
  metadata::{self, MetadataEdit},
  model::{ImageItem, SortSpec},
};

mod commands;
mod input;
mod navigation;
mod prompt;
mod prompt_input;
mod results;

pub use prompt::{CommandCompletion, EditorRequest, Prompt, PromptBuffer};
use prompt::{current_word_start, filter_completion_candidates};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
  Browser,
  Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailPage {
  Image,
  Metadata,
}

#[derive(Debug, Clone)]
pub enum ConfirmDialog {
  MetadataWrite { path: PathBuf, edit: MetadataEdit },
}

pub struct App {
  pub root: PathBuf,
  pub settings: Settings,
  pub keymap: KeyBindings,
  pub images: Vec<ImageItem>,
  pub focused: usize,
  pub selected: BTreeSet<PathBuf>,
  pub view: ViewMode,
  pub detail_page: DetailPage,
  pub browser_scroll: u32,
  pub detail_scroll: u16,
  pub last_layout: Option<BrowserLayout>,
  pub browser_viewport: Option<Rect>,
  pub browser_view_height: u16,
  pub hints: Vec<KeyHint>,
  pub pending_keys: Vec<String>,
  pub prompt: Option<Prompt>,
  pub command_history: Vec<String>,
  pub command_completion: Option<CommandCompletion>,
  pub message: String,
  pub sort_spec: SortSpec,
  pub scan_pending: bool,
  pub cache_clear_pending: bool,
  pub protocol_overlays: Vec<ProtocolOverlay>,
  pub terminal_cell_pixels: Option<(u16, u16)>,
  pub confirm: Option<ConfirmDialog>,
  quit: bool,
  stdout_paths: Option<Vec<PathBuf>>,
  editor_request: Option<EditorRequest>,
  command_history_index: Option<usize>,
  command_history_draft: Option<String>,
}

impl App {
  pub fn new(root: PathBuf, settings: Settings, images: Vec<ImageItem>) -> Self {
    let sort_spec = settings.config.initial_sort_spec();
    let keymap = KeyBindings::from_config(&settings.keymap);
    Self {
      root,
      settings,
      keymap,
      images,
      focused: 0,
      selected: BTreeSet::new(),
      view: ViewMode::Browser,
      detail_page: DetailPage::Image,
      browser_scroll: 0,
      detail_scroll: 0,
      last_layout: None,
      browser_viewport: None,
      browser_view_height: 1,
      hints: Vec::new(),
      pending_keys: Vec::new(),
      prompt: None,
      command_history: Vec::new(),
      command_completion: None,
      message: "ready".to_string(),
      sort_spec,
      scan_pending: false,
      cache_clear_pending: false,
      protocol_overlays: Vec::new(),
      terminal_cell_pixels: None,
      confirm: None,
      quit: false,
      stdout_paths: None,
      editor_request: None,
      command_history_index: None,
      command_history_draft: None,
    }
  }

  pub fn should_quit(&self) -> bool {
    self.quit
  }

  pub fn take_stdout_paths(&mut self) -> Option<Vec<PathBuf>> {
    self.stdout_paths.take()
  }

  pub fn take_editor_request(&mut self) -> Option<EditorRequest> {
    self.editor_request.take()
  }

  pub fn finish_prompt_editor_input(&mut self, result: Result<String, String>) {
    match result {
      Ok(input) => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().set_input(input);
          self.reset_command_history_cursor();
          self.refresh_command_completion();
          self.set_message("input updated from editor");
        }
      }
      Err(error) => self.set_message(format!("editor failed: {error}")),
    }
  }

  pub fn finish_metadata_editor_input(
    &mut self,
    path: PathBuf,
    original: Vec<crate::model::ImageMetadataEntry>,
    result: Result<String, String>,
  ) {
    let edited = match result {
      Ok(edited) => edited,
      Err(error) => {
        self.set_message(format!("editor failed: {error}"));
        return;
      }
    };
    let original_file_name = file_label(&path);
    let edit = match metadata::metadata_changes_from_edit(&original_file_name, &original, &edited) {
      Ok(edit) => edit,
      Err(error) => {
        self.set_message(format!("metadata edit failed: {error}"));
        return;
      }
    };
    if edit.is_empty() {
      self.set_message("metadata unchanged");
      return;
    }
    if let Some(change) = &edit.file_name
      && let Err(error) = validate_new_file_name(&path, &change.new_value)
    {
      self.set_message(error);
      return;
    }
    let count = edit.change_count();
    self.confirm = Some(ConfirmDialog::MetadataWrite { path, edit });
    self.set_message(format!("confirm metadata changes: {count} change(s)"));
  }

  pub fn set_message(&mut self, message: impl Into<String>) {
    let message = message.into();
    debug!(message, "status message");
    self.message = message;
  }

  pub fn update_browser_layout(&mut self, layout: BrowserLayout, viewport: Rect) {
    self.browser_viewport = Some(viewport);
    self.browser_view_height = viewport.height.max(1);
    let max_scroll = layout
      .total_height
      .saturating_sub(u32::from(self.browser_view_height));
    self.browser_scroll = self.browser_scroll.min(max_scroll);
    self.last_layout = Some(layout);
    if self.view == ViewMode::Browser {
      self.ensure_focus_visible();
    }
  }

  pub fn current(&self) -> Option<&ImageItem> {
    self.images.get(self.focused)
  }

  pub fn selected_or_focused_paths(&self) -> Vec<PathBuf> {
    if self.selected.is_empty() {
      self
        .current()
        .map(|item| vec![item.path.clone()])
        .unwrap_or_default()
    } else {
      self.selected.iter().cloned().collect()
    }
  }
}

fn is_safe_file_name(path: &Path, parent: &Path) -> bool {
  let Some(name) = path.file_name() else {
    return false;
  };
  path.parent() == Some(parent) && name != "." && name != ".."
}

fn validate_new_file_name(path: &Path, file_name: &str) -> Result<PathBuf, String> {
  if file_name.trim().is_empty() {
    return Err("filename cannot be empty".to_string());
  }
  if file_name.contains('\0') {
    return Err("filename cannot contain NUL".to_string());
  }
  let Some(parent) = path.parent() else {
    return Err("cannot rename path without parent".to_string());
  };
  let to = parent.join(file_name);
  if !is_safe_file_name(&to, parent) {
    return Err("rename must stay in the same directory".to_string());
  }
  if to != path && to.exists() {
    return Err(format!("target already exists: {}", file_label(&to)));
  }
  Ok(to)
}

fn rename_file_no_replace(from: &Path, to: &Path) -> Result<(), String> {
  if from == to {
    return Ok(());
  }
  rename_file_no_replace_impl(from, to)
}

#[cfg(target_os = "linux")]
fn rename_file_no_replace_impl(from: &Path, to: &Path) -> Result<(), String> {
  use std::{ffi::CString, os::unix::ffi::OsStrExt};

  let from_c = CString::new(from.as_os_str().as_bytes())
    .map_err(|_| "source path cannot contain NUL".to_string())?;
  let to_bytes = to.as_os_str().as_bytes();
  let to_c = CString::new(to_bytes).map_err(|_| "target path cannot contain NUL".to_string())?;

  let result = unsafe {
    libc::syscall(
      libc::SYS_renameat2,
      libc::AT_FDCWD,
      from_c.as_ptr(),
      libc::AT_FDCWD,
      to_c.as_ptr(),
      libc::RENAME_NOREPLACE,
    )
  };
  if result == 0 {
    return Ok(());
  }

  let error = std::io::Error::last_os_error();
  match error.raw_os_error() {
    Some(libc::EEXIST) => Err(format!("target already exists: {}", file_label(to))),
    Some(libc::ENOSYS) | Some(libc::EINVAL) => rename_file_no_replace_fallback(from, to),
    _ => Err(error.to_string()),
  }
}

#[cfg(not(target_os = "linux"))]
fn rename_file_no_replace_impl(from: &Path, to: &Path) -> Result<(), String> {
  rename_file_no_replace_fallback(from, to)
}

fn rename_file_no_replace_fallback(from: &Path, to: &Path) -> Result<(), String> {
  if to.exists() {
    return Err(format!("target already exists: {}", file_label(to)));
  }
  std::fs::rename(from, to).map_err(|err| err.to_string())
}

fn rename_cursor_position(file_name: &str) -> usize {
  file_name
    .rfind('.')
    .filter(|idx| *idx > 0)
    .unwrap_or(file_name.len())
}

fn file_label(path: &Path) -> String {
  path
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.display().to_string())
}

fn action_is_sort_command(action: &str) -> bool {
  action
    .split_whitespace()
    .next()
    .is_some_and(|command| command == "sort")
}

fn action_is_layout_command(action: &str) -> bool {
  action
    .split_whitespace()
    .next()
    .is_some_and(|command| matches!(command, "layout" | "layout-use"))
}

const COMMAND_NAMES: &[&str] = &["refresh", "clear-cache", "sort", "layout", "layout-use"];

#[cfg(test)]
mod tests;
