use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use framework_tui::{CommandState, KeyBindings, KeyDispatcher, KeyHint};
use ratatui::layout::Rect;
use tracing::debug;

use crate::{
  config::Settings,
  layout::BrowserLayout,
  metadata::{self, MetadataEdit},
  model::{ImageItem, ImageMetadataEntry, SortSpec, file_label},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEffect {
  None,
  Other,
  BrowseStep,
}

#[derive(Debug, Clone)]
pub enum ConfirmDialog {
  MetadataWrite { path: PathBuf, edit: MetadataEdit },
}

pub struct App {
  root: PathBuf,
  pub settings: Settings,
  keymap: KeyBindings,
  pub images: Vec<ImageItem>,
  pub focused: usize,
  pub selected: BTreeSet<PathBuf>,
  pub view: ViewMode,
  pub detail_page: DetailPage,
  pub browser_scroll: u32,
  last_layout: Option<BrowserLayout>,
  browser_viewport: Option<Rect>,
  browser_view_height: u16,
  pub prompt: Option<Prompt>,
  pub message: String,
  pub sort_spec: SortSpec,
  scan_pending: bool,
  cache_clear_pending: bool,
  pub terminal_cell_pixels: Option<(u16, u16)>,
  pub confirm: Option<ConfirmDialog>,
  pub key_help: bool,
  detail_back_quits: bool,
  /// Sort field completions built from the images' metadata, cleared when
  /// the image list or its metadata changes.
  metadata_fields: Option<Vec<String>>,
  quit: bool,
  stdout_paths: Option<Vec<PathBuf>>,
  editor_request: Option<EditorRequest>,
  command_state: CommandState,
  key_dispatcher: KeyDispatcher,
}

impl App {
  pub fn new(root: PathBuf, settings: Settings, images: Vec<ImageItem>) -> Self {
    let sort_spec = settings.config.initial_sort_spec();
    let keymap = settings.keymap.bindings();
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
      last_layout: None,
      browser_viewport: None,
      browser_view_height: 1,
      prompt: None,
      message: "ready".to_string(),
      sort_spec,
      scan_pending: false,
      cache_clear_pending: false,
      terminal_cell_pixels: None,
      confirm: None,
      key_help: false,
      detail_back_quits: false,
      metadata_fields: None,
      quit: false,
      stdout_paths: None,
      editor_request: None,
      command_state: CommandState::default(),
      key_dispatcher: KeyDispatcher::default(),
    }
  }

  pub fn should_quit(&self) -> bool {
    self.quit
  }

  pub fn request_quit(&mut self) {
    self.quit = true;
  }

  pub fn take_stdout_paths(&mut self) -> Option<Vec<PathBuf>> {
    self.stdout_paths.take()
  }

  pub fn take_editor_request(&mut self) -> Option<EditorRequest> {
    self.editor_request.take()
  }

  pub fn key_hints(&self) -> &[KeyHint] {
    self.key_dispatcher.hints()
  }

  pub fn command_completion(&self) -> Option<&CommandCompletion> {
    self.command_state.completion()
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
    original: Vec<ImageMetadataEntry>,
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

  pub fn editor_request_pending(&self) -> bool {
    self.editor_request.is_some()
  }

  /// Store the layout computed for this frame and keep the focused card
  /// visible. Returns whether the layout or viewport changed.
  pub fn update_browser_layout(&mut self, layout: BrowserLayout, viewport: Rect) -> bool {
    let changed = self.browser_viewport != Some(viewport)
      || self
        .last_layout
        .as_ref()
        .is_some_and(|previous| *previous != layout);
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
    changed
  }

  pub fn browser_layout(&self) -> Option<&BrowserLayout> {
    self.last_layout.as_ref()
  }

  pub fn current(&self) -> Option<&ImageItem> {
    self.images.get(self.focused)
  }

  pub fn enter_detail(&mut self, back_quits: bool) {
    if self.images.is_empty() {
      return;
    }
    self.view = ViewMode::Detail;
    self.detail_page = DetailPage::Image;
    self.detail_back_quits = back_quits;
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
  if to != path && entry_exists(&to) && !is_case_only_rename(path, &to) {
    return Err(format!("target already exists: {}", file_label(&to)));
  }
  Ok(to)
}

/// Whether anything, including a dangling symlink, exists at `path`.
fn entry_exists(path: &Path) -> bool {
  std::fs::symlink_metadata(path).is_ok()
}

/// Whether `to` differs from `from` only in letter case and both name the
/// same file, as on the case-insensitive file systems macOS and Windows use
/// by default. Such a rename must not be refused as "target exists".
fn is_case_only_rename(from: &Path, to: &Path) -> bool {
  let (Some(from_name), Some(to_name)) = (from.file_name(), to.file_name()) else {
    return false;
  };
  from_name != to_name
    && from_name.to_string_lossy().to_lowercase() == to_name.to_string_lossy().to_lowercase()
    && same_file(from, to)
}

#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
  use std::os::unix::fs::MetadataExt;
  match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
    (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
    _ => false,
  }
}

#[cfg(not(unix))]
fn same_file(a: &Path, b: &Path) -> bool {
  // Canonical paths carry the on-disk letter case, so two spellings of one
  // file resolve to the same path.
  match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
    (Ok(a), Ok(b)) => a == b,
    _ => false,
  }
}

fn rename_file_no_replace(from: &Path, to: &Path) -> Result<(), String> {
  if from == to {
    return Ok(());
  }
  if is_case_only_rename(from, to) {
    // `to` is `from` itself, so there is nothing to protect from replacement.
    return std::fs::rename(from, to).map_err(|err| err.to_string());
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
  if entry_exists(to) {
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

const COMMAND_NAMES: &[&str] = &[
  "refresh",
  "clear-cache",
  "sort",
  "layout",
  "layout-use",
  "help",
];

/// Commands that take no arguments.
const NO_ARG_COMMANDS: &[&str] = &["refresh", "clear-cache", "help"];

#[cfg(all(test, unix))]
mod tests {
  use std::fs;

  use super::*;

  fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
      "gallery-tui-rename-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
  }

  #[test]
  fn rename_validation_allows_case_changes_of_the_same_file_only() {
    let dir = temp_dir();
    let image = dir.join("photo.JPG");
    fs::write(&image, b"x").unwrap();
    // A hard link stands in for a case-insensitive file system: a second name
    // that differs only in case and refers to the same file.
    fs::hard_link(&image, dir.join("photo.jpg")).unwrap();
    fs::write(dir.join("other.jpg"), b"y").unwrap();
    fs::write(dir.join("OTHER.JPG"), b"z").unwrap();
    std::os::unix::fs::symlink(dir.join("missing"), dir.join("dangling.jpg")).unwrap();

    let case_change = validate_new_file_name(&image, "photo.jpg");
    let other = validate_new_file_name(&dir.join("other.jpg"), "OTHER.JPG");
    let dangling = validate_new_file_name(&image, "dangling.jpg");
    let outside = validate_new_file_name(&image, "../escape.jpg");
    fs::remove_dir_all(&dir).unwrap();

    assert_eq!(case_change, Ok(dir.join("photo.jpg")));
    assert!(other.is_err());
    assert!(dangling.is_err());
    assert!(outside.is_err());
  }
}
