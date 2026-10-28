use crate::rename::{rename_file_no_replace, validate_new_file_name};

use std::{collections::BTreeSet, path::PathBuf};

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
mod composition;
pub(crate) use composition::BrowserPanel;
mod editor;
mod input;
mod navigation;
mod prompt_input;
mod results;

pub use editor::EditorRequest;
pub use framework_tui::{CommandCompletion, Prompt, PromptBuffer};
use framework_tui::{current_word_start, filter_completion_candidates};

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

#[derive(PartialEq)]
struct BrowserLayoutKey {
  area: Rect,
  config: crate::config::EffectiveLayoutConfig,
  cell_pixels: Option<(u16, u16)>,
}

pub struct App {
  pub(crate) overlay_areas: Vec<Rect>,
  root: PathBuf,
  pub settings: Settings,
  keymap: KeyBindings,
  pub images: Vec<ImageItem>,
  pub focused: usize,
  pub selected: BTreeSet<PathBuf>,
  pub view: ViewMode,
  pub detail_page: DetailPage,
  pub(crate) metadata_scroll: u16,
  pub(crate) metadata_max_scroll: u16,
  pub(crate) metadata_view_height: u16,
  pub browser_scroll: u32,
  last_layout: Option<BrowserLayout>,
  layout_key: Option<BrowserLayoutKey>,
  pub(crate) follower: Option<BrowserPanel>,
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
      overlay_areas: Vec::new(),
      root,
      settings,
      keymap,
      images,
      focused: 0,
      selected: BTreeSet::new(),
      view: ViewMode::Browser,
      detail_page: DetailPage::Image,
      metadata_scroll: 0,
      metadata_max_scroll: 0,
      metadata_view_height: 1,
      browser_scroll: 0,
      last_layout: None,
      layout_key: None,
      follower: None,
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

  pub(super) fn invalidate_layout(&mut self) {
    self.layout_key = None;
    self.follower = None;
  }
  pub fn prepare_browser_layout(
    &mut self,
    config: &crate::config::EffectiveLayoutConfig,
    area: Rect,
  ) -> bool {
    let key = BrowserLayoutKey {
      area,
      config: config.clone(),
      cell_pixels: self.terminal_cell_pixels,
    };
    if self.layout_key.as_ref() == Some(&key) {
      return false;
    }
    let layout =
      crate::layout::compute_browser_layout(&self.images, area, config, self.terminal_cell_pixels);
    self.layout_key = Some(key);
    self.update_browser_layout(layout, area);
    true
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

  pub fn browser_viewport(&self) -> Option<Rect> {
    self.browser_viewport
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
