use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
  cache,
  config::{self, Settings},
  event::{
    AsyncEvent, CacheClearOutcome, ConfigSaveOutcome, MetadataWriteOutcome, ProtocolOverlay,
    RenameOutcome, ScanOutcome,
  },
  keymap::{KeyBindings, KeyContext, KeyHint, MatchResult, key_event_to_token},
  layout::BrowserLayout,
  metadata::{self, MetadataEdit},
  model::{ImageItem, SortSpec, sort_images},
  scanner,
};

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
pub enum Prompt {
  Rename { buffer: PromptBuffer },
  Command { buffer: PromptBuffer },
}

#[derive(Debug, Clone)]
pub struct PromptBuffer {
  pub input: String,
  pub cursor: usize,
}

#[derive(Debug, Clone)]
pub enum EditorRequest {
  Prompt {
    input: String,
  },
  Metadata {
    path: PathBuf,
    original: Vec<crate::model::ImageMetadataEntry>,
    draft: String,
  },
}

impl EditorRequest {
  pub fn initial_text(&self) -> &str {
    match self {
      EditorRequest::Prompt { input } => input,
      EditorRequest::Metadata { draft, .. } => draft,
    }
  }
}

#[derive(Debug, Clone)]
pub enum ConfirmDialog {
  MetadataWrite { path: PathBuf, edit: MetadataEdit },
}

impl PromptBuffer {
  fn new(input: impl Into<String>) -> Self {
    let input = input.into();
    let cursor = input.len();
    Self { input, cursor }
  }

  fn set_input(&mut self, input: String) {
    self.input = input;
    self.cursor = self.input.len();
  }

  fn insert_char(&mut self, ch: char) {
    self.input.insert(self.cursor, ch);
    self.cursor += ch.len_utf8();
  }

  fn backspace(&mut self) {
    if self.cursor == 0 {
      return;
    }
    let previous = previous_boundary(&self.input, self.cursor);
    self.input.drain(previous..self.cursor);
    self.cursor = previous;
  }

  fn delete(&mut self) {
    if self.cursor >= self.input.len() {
      return;
    }
    let next = next_boundary(&self.input, self.cursor);
    self.input.drain(self.cursor..next);
  }

  fn move_left(&mut self) {
    self.cursor = previous_boundary(&self.input, self.cursor);
  }

  fn move_right(&mut self) {
    self.cursor = next_boundary(&self.input, self.cursor);
  }

  fn move_start(&mut self) {
    self.cursor = 0;
  }

  fn move_end(&mut self) {
    self.cursor = self.input.len();
  }

  fn kill_before_cursor(&mut self) {
    self.input.drain(..self.cursor);
    self.cursor = 0;
  }

  fn kill_after_cursor(&mut self) {
    self.input.truncate(self.cursor);
  }

  pub fn cursor_columns(&self) -> usize {
    self.input[..self.cursor].chars().count()
  }
}

impl Prompt {
  fn rename(input: impl Into<String>) -> Self {
    Self::Rename {
      buffer: PromptBuffer::new(input),
    }
  }

  fn command(input: impl Into<String>) -> Self {
    Self::Command {
      buffer: PromptBuffer::new(input),
    }
  }

  pub fn buffer(&self) -> &PromptBuffer {
    match self {
      Prompt::Rename { buffer } | Prompt::Command { buffer } => buffer,
    }
  }

  fn buffer_mut(&mut self) -> &mut PromptBuffer {
    match self {
      Prompt::Rename { buffer } | Prompt::Command { buffer } => buffer,
    }
  }
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

  pub fn handle_input(&mut self, input: Event, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.confirm.is_some() {
      self.handle_confirm_input(input, tx);
      return;
    }
    match input {
      Event::Key(key) if self.prompt.is_some() => self.handle_prompt_key(key, tx),
      Event::Key(key) => {
        let Some(token) = key_event_to_token(key) else {
          return;
        };
        self.handle_key_token(token, tx);
      }
      Event::Mouse(mouse) => match mouse.kind {
        MouseEventKind::ScrollDown => self.handle_scroll_down(),
        MouseEventKind::ScrollUp => self.handle_scroll_up(),
        MouseEventKind::Down(MouseButton::Left) => self.handle_mouse_click(mouse.column, mouse.row),
        _ => {}
      },
      Event::Resize(_, _) => {}
      _ => {}
    }
  }

  pub fn finish_scan(&mut self, outcome: ScanOutcome) {
    self.scan_pending = false;
    match outcome.result {
      Ok(mut images) => {
        sort_images(&mut images, &outcome.sort);
        self.images = images;
        self.sort_spec = outcome.sort.clone();
        self.restore_focus(outcome.preserve_focus.as_deref());
        self
          .selected
          .retain(|path| self.images.iter().any(|item| item.path == *path));
        self.set_message(format!("refreshed {} images", self.images.len()));
        info!(count = self.images.len(), "scan finished");
      }
      Err(error) => {
        error!(error, "scan failed");
        self.set_message(format!("refresh failed: {error}"));
      }
    }
  }

  pub fn finish_rename(&mut self, outcome: RenameOutcome) {
    match outcome.result {
      Ok(()) => {
        for item in &mut self.images {
          if item.path == outcome.from {
            item.path = outcome.to.clone();
            item.refresh_name();
            break;
          }
        }
        if self.selected.remove(&outcome.from) {
          self.selected.insert(outcome.to.clone());
        }
        self.set_message(format!("renamed to {}", file_label(&outcome.to)));
        info!(from = %outcome.from.display(), to = %outcome.to.display(), "rename finished");
      }
      Err(error) => {
        error!(from = %outcome.from.display(), to = %outcome.to.display(), error, "rename failed");
        self.set_message(format!("rename failed: {error}"));
      }
    }
  }

  pub fn finish_cache_clear(&mut self, outcome: CacheClearOutcome) {
    self.cache_clear_pending = false;
    match outcome.result {
      Ok(report) => {
        self.set_message(format!(
          "cleared cache: {} files, {}",
          report.removed_files,
          humansize::format_size(report.removed_bytes, humansize::DECIMAL)
        ));
        info!(
          removed_files = report.removed_files,
          removed_bytes = report.removed_bytes,
          "render cache cleared"
        );
      }
      Err(error) => {
        error!(error, "cache clear failed");
        self.set_message(format!("clear-cache failed: {error}"));
      }
    }
  }

  pub fn finish_config_save(&mut self, outcome: ConfigSaveOutcome) {
    match outcome.result {
      Ok(message) => {
        self.set_message(message);
        info!("config saved");
      }
      Err(error) => {
        error!(error, "config save failed");
        self.set_message(format!("config save failed: {error}"));
      }
    }
  }

  pub fn finish_metadata_write(&mut self, outcome: MetadataWriteOutcome) {
    if outcome.rename_applied {
      for item in &mut self.images {
        if item.path == outcome.from {
          item.path = outcome.to.clone();
          item.refresh_name();
          break;
        }
      }
      if self.selected.remove(&outcome.from) {
        self.selected.insert(outcome.to.clone());
      }
    }

    let current_path = if outcome.rename_applied {
      &outcome.to
    } else {
      &outcome.from
    };
    match outcome.result {
      Ok(metadata) => {
        if let Some(item) = self
          .images
          .iter_mut()
          .find(|item| item.path == *current_path)
        {
          item.metadata = metadata;
        }
        if outcome.edit.file_name.is_some() && outcome.edit.tags.is_empty() {
          self.set_message(format!("renamed to {}", file_label(&outcome.to)));
        } else {
          self.set_message(format!(
            "metadata updated: {} tag(s){}",
            outcome.edit.tags.len(),
            if outcome.edit.file_name.is_some() {
              " and filename"
            } else {
              ""
            }
          ));
        }
        info!(
          from = %outcome.from.display(),
          to = %outcome.to.display(),
          tags = outcome.edit.tags.len(),
          renamed = outcome.rename_applied,
          "metadata write finished"
        );
      }
      Err(error) => {
        error!(from = %outcome.from.display(), to = %outcome.to.display(), %error, "metadata write failed");
        if outcome.rename_applied {
          self.set_message(format!("metadata write failed after rename: {error}"));
        } else {
          self.set_message(format!("metadata write failed: {error}"));
        }
      }
    }
  }

  fn handle_confirm_input(&mut self, input: Event, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let Event::Key(key) = input else {
      return;
    };
    let Some(token) = key_event_to_token(key) else {
      return;
    };
    match token.as_str() {
      "y" => self.apply_confirm(tx),
      "enter" | "n" | "q" | "esc" => {
        self.confirm = None;
        self.set_message("cancelled");
      }
      _ => {}
    }
  }

  fn apply_confirm(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let Some(confirm) = self.confirm.take() else {
      return;
    };
    match confirm {
      ConfirmDialog::MetadataWrite { path, edit } => {
        let to = match edit
          .file_name
          .as_ref()
          .map(|change| validate_new_file_name(&path, &change.new_value))
          .transpose()
        {
          Ok(Some(to)) => to,
          Ok(None) => path.clone(),
          Err(error) => {
            self.set_message(error);
            return;
          }
        };
        let tx = tx.clone();
        self.set_message(format!(
          "applying metadata edit: {} change(s)",
          edit.change_count()
        ));
        tokio::task::spawn_blocking(move || {
          let mut rename_applied = false;
          let result = (|| {
            if path != to {
              std::fs::rename(&path, &to).map_err(|err| err.to_string())?;
              rename_applied = true;
            }
            metadata::write_metadata_with_exiftool(&to, &edit.tags)?;
            Ok(metadata::refresh_metadata_after_write(to.clone()))
          })();
          let _ = tx.send(AsyncEvent::MetadataWrite(MetadataWriteOutcome {
            from: path,
            to,
            result,
            edit,
            rename_applied,
          }));
        });
      }
    }
  }

  fn handle_key_token(&mut self, token: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.view == ViewMode::Detail && token == "q" {
      self.pending_keys.clear();
      self.hints.clear();
      self.handle_back();
      return;
    }

    let mut sequence = self.pending_keys.clone();
    sequence.push(token.clone());
    match self.keymap.match_sequence(self.key_context(), &sequence) {
      MatchResult::Action(action) => {
        self.pending_keys.clear();
        self.hints.clear();
        self.handle_action(&action, tx);
      }
      MatchResult::Prefix(hints) => {
        self.pending_keys = sequence;
        self.hints = hints;
      }
      MatchResult::None if !self.pending_keys.is_empty() => {
        self.pending_keys.clear();
        self.hints.clear();
        match self
          .keymap
          .match_sequence(self.key_context(), &[token.clone()])
        {
          MatchResult::Action(action) => self.handle_action(&action, tx),
          MatchResult::Prefix(hints) => {
            self.pending_keys = vec![token];
            self.hints = hints;
          }
          MatchResult::None => {}
        }
      }
      MatchResult::None => {
        self.hints.clear();
      }
    }
  }

  fn key_context(&self) -> KeyContext {
    match self.view {
      ViewMode::Browser => KeyContext::Browser,
      ViewMode::Detail => KeyContext::Detail,
    }
  }

  fn action_available(&self, action: &str) -> bool {
    if action == "quit" {
      return self.view == ViewMode::Browser;
    }
    if matches!(action, "rename" | "command") {
      return true;
    }
    if action_is_sort_command(action) {
      return self.view == ViewMode::Browser;
    }
    if action_is_layout_command(action) {
      return self.view == ViewMode::Browser;
    }
    match self.view {
      ViewMode::Browser => matches!(
        action,
        "open"
          | "move_left"
          | "move_down"
          | "move_up"
          | "move_right"
          | "page_up"
          | "page_down"
          | "home"
          | "end"
          | "toggle_select"
          | "copy_paths"
      ),
      ViewMode::Detail => matches!(
        action,
        "back" | "move_left" | "move_down" | "move_up" | "move_right" | "edit_metadata"
      ),
    }
  }

  fn handle_action(&mut self, action: &str, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if !self.action_available(action) {
      self.set_message(format!("not available here: {action}"));
      return;
    }
    if action_is_sort_command(action) {
      self.execute_command(action.to_string(), tx);
      return;
    }
    if action_is_layout_command(action) {
      self.execute_command(action.to_string(), tx);
      return;
    }

    match action {
      "quit" => self.quit = true,
      "back" => self.handle_back(),
      "open" => {
        if self.view == ViewMode::Browser && !self.images.is_empty() {
          self.view = ViewMode::Detail;
          self.detail_scroll = 0;
        }
      }
      "move_left" => self.move_left(),
      "move_down" => self.move_down(),
      "move_up" => self.move_up(),
      "move_right" => self.move_right(),
      "page_up" => self.page_up(),
      "page_down" => self.page_down(),
      "home" => self.focus_first(),
      "end" => self.focus_last(),
      "toggle_select" => self.toggle_select(),
      "rename" => self.start_rename(),
      "command" => self.start_command(),
      "edit_metadata" => self.start_metadata_edit(),
      "copy_paths" => {
        self.stdout_paths = Some(self.selected_or_focused_paths());
        self.quit = true;
      }
      other => {
        warn!(other, "unknown action");
        self.set_message(format!("unknown action: {other}"));
      }
    }

    let _ = tx;
  }

  fn handle_back(&mut self) {
    if self.view == ViewMode::Detail {
      self.view = ViewMode::Browser;
    } else {
      self.pending_keys.clear();
      self.hints.clear();
    }
  }

  fn move_left(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_horizontal(-1),
      ViewMode::Detail => self.detail_page = DetailPage::Image,
    }
  }

  fn move_right(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_horizontal(1),
      ViewMode::Detail => self.detail_page = DetailPage::Metadata,
    }
  }

  fn move_down(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(1),
      ViewMode::Detail => self.focus_relative(1),
    }
  }

  fn move_up(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(-1),
      ViewMode::Detail => self.focus_relative(-1),
    }
  }

  fn page_up(&mut self) {
    match self.view {
      ViewMode::Browser => {
        let count = self.page_item_count();
        self.focus_relative(-(count as isize));
      }
      ViewMode::Detail => self.focus_relative(-1),
    }
  }

  fn page_down(&mut self) {
    match self.view {
      ViewMode::Browser => {
        let count = self.page_item_count();
        self.focus_relative(count as isize);
      }
      ViewMode::Detail => self.focus_relative(1),
    }
  }

  fn focus_first(&mut self) {
    self.focused = 0;
    self.ensure_focus_visible();
  }

  fn focus_last(&mut self) {
    if !self.images.is_empty() {
      self.focused = self.images.len() - 1;
      self.ensure_focus_visible();
    }
  }

  fn focus_relative(&mut self, delta: isize) {
    if self.images.is_empty() {
      self.focused = 0;
      return;
    }
    let max = self.images.len() - 1;
    self.focused = self.focused.saturating_add_signed(delta).min(max);
    self.ensure_focus_visible();
  }

  fn focus_horizontal(&mut self, delta: isize) {
    let Some(layout) = &self.last_layout else {
      self.focus_relative(delta);
      return;
    };
    if layout.columns <= 1 {
      self.focus_relative(delta);
      return;
    }
    let col = self.focused % layout.columns;
    if delta < 0 && col == 0 {
      return;
    }
    if delta > 0 && col + 1 >= layout.columns {
      return;
    }
    self.focus_relative(delta);
  }

  fn focus_vertical(&mut self, delta: isize) {
    let Some(layout) = &self.last_layout else {
      self.focus_relative(delta);
      return;
    };
    if layout.cards.len() != self.images.len() || self.focused >= layout.cards.len() {
      self.focus_relative(delta * layout.columns as isize);
      return;
    }

    let current = layout.cards[self.focused];
    let target = layout
      .cards
      .iter()
      .enumerate()
      .filter(|(idx, card)| {
        if delta > 0 {
          *idx != self.focused && card.y > current.y
        } else {
          *idx != self.focused && card.y < current.y
        }
      })
      .min_by_key(|(_, card)| {
        let dy = card.y.abs_diff(current.y);
        let dx = (card.center_x() - current.center_x()).unsigned_abs();
        dy.saturating_mul(1000).saturating_add(dx)
      })
      .map(|(idx, _)| idx);
    if let Some(idx) = target {
      self.focused = idx;
      self.ensure_focus_visible();
    }
  }

  fn page_item_count(&self) -> usize {
    let Some(layout) = &self.last_layout else {
      return 1;
    };
    let viewport_top = self.browser_scroll;
    let viewport_bottom = self
      .browser_scroll
      .saturating_add(u32::from(self.browser_view_height));
    layout
      .cards
      .iter()
      .filter(|card| {
        let card_top = card.y;
        let card_bottom = card.y.saturating_add(u32::from(card.height));
        card_bottom > viewport_top && card_top < viewport_bottom
      })
      .count()
      .max(1)
  }

  fn toggle_select(&mut self) {
    let Some(path) = self.current().map(|item| item.path.clone()) else {
      return;
    };
    if !self.selected.remove(&path) {
      self.selected.insert(path);
    }
    if self.settings.config.behavior.select_moves_focus && self.focused + 1 < self.images.len() {
      self.focused += 1;
      self.ensure_focus_visible();
    }
  }

  fn start_rename(&mut self) {
    let Some(file_name) = self.current().map(|item| item.file_name.clone()) else {
      return;
    };
    self.command_completion = None;
    let mut prompt = Prompt::rename(file_name);
    let cursor = rename_cursor_position(&prompt.buffer().input);
    prompt.buffer_mut().cursor = cursor;
    self.prompt = Some(prompt);
  }

  fn start_command(&mut self) {
    self.command_history_index = None;
    self.command_history_draft = None;
    self.prompt = Some(Prompt::command(String::new()));
    self.refresh_command_completion();
  }

  fn start_metadata_edit(&mut self) {
    if self.view != ViewMode::Detail {
      self.set_message("metadata edit is only available in detail view");
      return;
    }
    let Some(item) = self.current() else {
      return;
    };
    let draft = metadata::metadata_edit_draft(item);
    self.editor_request = Some(EditorRequest::Metadata {
      path: item.path.clone(),
      original: item.metadata.clone(),
      draft,
    });
    self.set_message("editing metadata");
  }

  fn apply_sort(&mut self, sort_spec: SortSpec) {
    let focused_path = self.current().map(|item| item.path.clone());
    sort_images(&mut self.images, &sort_spec);
    self.sort_spec = sort_spec.clone();
    self.restore_focus(focused_path.as_deref());
    self.set_message(format!("sort: {}", sort_spec.label()));
    info!(sort = sort_spec.label(), "sort changed");
  }

  fn request_refresh(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.scan_pending {
      self.set_message("refresh already running");
      return;
    }
    self.scan_pending = true;
    let root = self.root.clone();
    let config = self.settings.config.clone();
    let sort = self.sort_spec.clone();
    let preserve_focus = self.current().map(|item| item.path.clone());
    let tx = tx.clone();
    info!(root = %root.display(), "refresh requested");
    tokio::spawn(async move {
      let result = scanner::scan_images(root, &config)
        .await
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::Scan(ScanOutcome {
        result,
        preserve_focus,
        sort,
      }));
    });
  }

  fn request_rename(&mut self, input: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let Some(item) = self.current() else {
      return;
    };
    let from = item.path.clone();
    let to = match validate_new_file_name(&from, &input) {
      Ok(to) => to,
      Err(error) => {
        self.set_message(error);
        return;
      }
    };
    if from == to {
      self.set_message("rename unchanged");
      return;
    }
    let tx = tx.clone();
    info!(from = %from.display(), to = %to.display(), "rename requested");
    tokio::spawn(async move {
      let result = tokio::fs::rename(&from, &to)
        .await
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::Rename(RenameOutcome { from, to, result }));
    });
  }

  fn handle_prompt_key(
    &mut self,
    key: crossterm::event::KeyEvent,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
  ) {
    if key.kind != KeyEventKind::Press {
      return;
    }

    if let Some(token) = key_event_to_token(key) {
      match self.keymap.match_sequence(KeyContext::Input, &[token]) {
        MatchResult::Action(action) => {
          self.handle_prompt_action(&action, tx);
          return;
        }
        MatchResult::Prefix(_) => return,
        MatchResult::None => {}
      }
    }

    match key.code {
      KeyCode::Char(ch)
        if key.modifiers.is_empty() || key.modifiers == crossterm::event::KeyModifiers::SHIFT =>
      {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().insert_char(ch);
        }
        self.reset_command_history_cursor();
        self.refresh_command_completion();
      }
      _ => {}
    }
  }

  fn handle_prompt_action(&mut self, action: &str, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    match action {
      "cancel" => self.cancel_prompt(),
      "submit" => self.submit_prompt(tx),
      "backspace" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().backspace();
        }
        self.reset_command_history_cursor();
        self.refresh_command_completion();
      }
      "delete" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().delete();
        }
        self.reset_command_history_cursor();
        self.refresh_command_completion();
      }
      "move_left" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().move_left();
        }
        self.refresh_command_completion();
      }
      "move_right" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().move_right();
        }
        self.refresh_command_completion();
      }
      "move_start" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().move_start();
        }
        self.refresh_command_completion();
      }
      "move_end" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().move_end();
        }
        self.refresh_command_completion();
      }
      "kill_before_cursor" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().kill_before_cursor();
        }
        self.reset_command_history_cursor();
        self.refresh_command_completion();
      }
      "kill_after_cursor" => {
        if let Some(prompt) = self.prompt.as_mut() {
          prompt.buffer_mut().kill_after_cursor();
        }
        self.reset_command_history_cursor();
        self.refresh_command_completion();
      }
      "completion_next" => self.select_next_completion(),
      "completion_previous" => self.select_previous_completion(),
      "history_previous" => self.command_history_previous(),
      "history_next" => self.command_history_next(),
      "edit_in_editor" => {
        if let Some(prompt) = &self.prompt {
          self.editor_request = Some(EditorRequest::Prompt {
            input: prompt.buffer().input.clone(),
          });
          self.command_completion = None;
        }
      }
      other => self.set_message(format!("unknown input action: {other}")),
    }
  }

  fn cancel_prompt(&mut self) {
    self.prompt = None;
    self.command_history_index = None;
    self.command_history_draft = None;
    self.command_completion = None;
    self.set_message("cancelled");
  }

  fn submit_prompt(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.complete_selected_command_candidate() {
      return;
    }
    let prompt = self.prompt.take();
    self.command_completion = None;
    match prompt {
      Some(Prompt::Rename { buffer }) => self.request_rename(buffer.input, tx),
      Some(Prompt::Command { buffer }) => self.submit_command(buffer.input, tx),
      None => {}
    }
  }

  fn execute_command(&mut self, input: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let command = input.trim().trim_start_matches(':');
    let mut parts = command.split_whitespace();
    match parts.next() {
      Some("refresh") if parts.next().is_none() => self.request_refresh(tx),
      Some("clear-cache") if parts.next().is_none() => self.request_clear_cache(tx),
      Some("sort") => self.execute_sort_command(parts.collect()),
      Some("layout") => self.execute_layout_command(parts.collect(), true, tx),
      Some("layout-use") => self.execute_layout_command(parts.collect(), false, tx),
      Some("") | None => self.set_message("empty command"),
      Some(other) => self.set_message(format!("unknown command: {other}")),
    }
  }

  fn execute_sort_command(&mut self, args: Vec<&str>) {
    if args.len() < 2 {
      self.set_message("usage: :sort <field> <asc|desc>");
      return;
    }
    let direction = args[args.len() - 1];
    let field = args[..args.len() - 1].join(" ");
    let Some(sort_spec) = config::sort_for_command(&field, direction) else {
      self.set_message("usage: :sort <field> <asc|desc>");
      return;
    };
    self.apply_sort(sort_spec);
  }

  fn execute_layout_command(
    &mut self,
    args: Vec<&str>,
    persist: bool,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
  ) {
    let Some(name) = args.first().copied() else {
      self.set_message("usage: :layout <name> [args...]");
      return;
    };
    match self
      .settings
      .config
      .layout
      .set_active_from_args(name, &args[1..])
    {
      Ok(layout) => {
        self.last_layout = None;
        let label = layout.label();
        if persist {
          self.set_message(format!("layout: {label} (saving)"));
          self.request_config_save(format!("layout saved: {label}"), tx);
        } else {
          self.set_message(format!("layout: {label} (temporary)"));
        }
        info!(layout = label, persist, "layout changed");
      }
      Err(error) => self.set_message(error),
    }
  }

  fn submit_command(&mut self, input: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let command = input.trim().trim_start_matches(':').trim().to_string();
    if !command.is_empty() && self.command_history.last() != Some(&command) {
      self.command_history.push(command.clone());
    }
    self.command_history_index = None;
    self.command_history_draft = None;
    self.execute_command(command, tx);
  }

  fn request_config_save(&self, success_message: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let path = self.settings.config_path.clone();
    let config = self.settings.config.clone();
    let tx = tx.clone();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
      handle.spawn(async move {
        let result = config::write_app_config(&path, &config)
          .await
          .map(|()| success_message)
          .map_err(|err| err.to_string());
        let _ = tx.send(AsyncEvent::ConfigSave(ConfigSaveOutcome { result }));
      });
    } else {
      let result = config::write_app_config_sync(&path, &config)
        .map(|()| success_message)
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::ConfigSave(ConfigSaveOutcome { result }));
    }
  }

  fn command_buffer(&self) -> Option<&PromptBuffer> {
    match self.prompt.as_ref()? {
      Prompt::Command { buffer } => Some(buffer),
      Prompt::Rename { .. } => None,
    }
  }

  fn command_buffer_mut(&mut self) -> Option<&mut PromptBuffer> {
    match self.prompt.as_mut()? {
      Prompt::Command { buffer } => Some(buffer),
      Prompt::Rename { .. } => None,
    }
  }

  fn set_command_input(&mut self, value: String) {
    if let Some(buffer) = self.command_buffer_mut() {
      buffer.set_input(value);
    }
  }

  fn reset_command_history_cursor(&mut self) {
    if self.command_buffer().is_some() {
      self.command_history_index = None;
      self.command_history_draft = None;
    }
  }

  fn command_history_previous(&mut self) {
    let Some(current_input) = self.command_buffer().map(|buffer| buffer.input.clone()) else {
      return;
    };
    if self.command_history.is_empty() {
      return;
    }
    let index = match self.command_history_index {
      Some(0) => 0,
      Some(index) => index.saturating_sub(1),
      None => {
        self.command_history_draft = Some(current_input);
        self.command_history.len() - 1
      }
    };
    self.command_history_index = Some(index);
    self.set_command_input(self.command_history[index].clone());
    self.refresh_command_completion();
  }

  fn command_history_next(&mut self) {
    let Some(index) = self.command_history_index else {
      return;
    };
    if index + 1 < self.command_history.len() {
      let next = index + 1;
      self.command_history_index = Some(next);
      self.set_command_input(self.command_history[next].clone());
    } else {
      self.command_history_index = None;
      let draft = self.command_history_draft.take().unwrap_or_default();
      self.set_command_input(draft);
    }
    self.refresh_command_completion();
  }

  fn refresh_command_completion(&mut self) {
    let Some(buffer) = self.command_buffer() else {
      self.command_completion = None;
      return;
    };
    let input = buffer.input.clone();
    let cursor = buffer.cursor;

    let previous = self.command_completion.clone();
    let Some(mut completion) = self.command_completion_for(&input, cursor) else {
      self.command_completion = None;
      return;
    };
    if completion.candidates.is_empty() {
      self.command_completion = None;
      return;
    }
    if let Some(previous) = previous {
      if let Some(candidate) = previous.selected_candidate()
        && let Some(index) = completion
          .candidates
          .iter()
          .position(|value| value == candidate)
      {
        completion.selected = index;
      }
    }
    self.command_completion = Some(completion);
  }

  fn select_next_completion(&mut self) {
    self.refresh_command_completion();
    let Some(completion) = self.command_completion.as_mut() else {
      return;
    };
    if completion.candidates.is_empty() {
      return;
    }
    completion.selected = (completion.selected + 1) % completion.candidates.len();
  }

  fn select_previous_completion(&mut self) {
    self.refresh_command_completion();
    let Some(completion) = self.command_completion.as_mut() else {
      return;
    };
    if completion.candidates.is_empty() {
      return;
    }
    completion.selected =
      (completion.selected + completion.candidates.len() - 1) % completion.candidates.len();
  }

  fn complete_selected_command_candidate(&mut self) -> bool {
    let Some(buffer) = self.command_buffer() else {
      return false;
    };
    let input = buffer.input.clone();
    self.refresh_command_completion();
    let Some(completion) = self.command_completion.clone() else {
      return false;
    };
    let Some(candidate) = completion.selected_candidate().cloned() else {
      return false;
    };
    let current = input
      .get(completion.replace_start..completion.replace_end)
      .unwrap_or_default();
    let mut next = input[..completion.replace_start].to_string();
    next.push_str(&candidate);
    if completion.append_space && !next.ends_with(' ') {
      next.push(' ');
    }
    let next_cursor = next.len();
    next.push_str(input.get(completion.replace_end..).unwrap_or_default());
    if next == input || (current == candidate && !completion.append_space) {
      return false;
    }
    if let Some(buffer) = self.command_buffer_mut() {
      buffer.input = next;
      buffer.cursor = next_cursor.min(buffer.input.len());
    }
    self.reset_command_history_cursor();
    self.refresh_command_completion();
    true
  }

  fn command_completion_for(&self, input: &str, cursor: usize) -> Option<CommandCompletion> {
    let cursor = cursor.min(input.len());
    let before_cursor = input.get(..cursor)?;
    let normalized = before_cursor.trim_start_matches(':');
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    let ends_with_space = normalized.chars().last().is_some_and(char::is_whitespace);
    let word_start = current_word_start(input, cursor);
    let prefix = if ends_with_space {
      ""
    } else {
      input.get(word_start..cursor).unwrap_or_default()
    };

    if tokens.is_empty() || (tokens.len() == 1 && !ends_with_space) {
      return Some(CommandCompletion::new(
        word_start,
        cursor,
        prefix,
        filter_completion_candidates(COMMAND_NAMES.iter().copied(), prefix),
        true,
        0,
      ));
    }

    match tokens[0] {
      "layout" | "layout-use" => {
        if tokens.len() > 2 || (tokens.len() == 2 && ends_with_space) {
          return None;
        }
        let replace_start = if ends_with_space { cursor } else { word_start };
        let prefix = if ends_with_space { "" } else { prefix };
        Some(CommandCompletion::new(
          replace_start,
          cursor,
          prefix,
          filter_completion_candidates(self.settings.config.layout.presets.keys(), prefix),
          true,
          0,
        ))
      }
      "sort" => {
        if ends_with_space && tokens.len() == 1 {
          return Some(CommandCompletion::new(
            cursor,
            cursor,
            "",
            self.sort_field_completions(""),
            true,
            0,
          ));
        }
        if !ends_with_space && tokens.len() <= 2 {
          return Some(CommandCompletion::new(
            word_start,
            cursor,
            prefix,
            self.sort_field_completions(prefix),
            true,
            0,
          ));
        }
        let replace_start = if ends_with_space { cursor } else { word_start };
        let prefix = if ends_with_space { "" } else { prefix };
        Some(CommandCompletion::new(
          replace_start,
          cursor,
          prefix,
          filter_completion_candidates(["asc", "desc"], prefix),
          false,
          0,
        ))
      }
      _ => None,
    }
  }

  fn sort_field_completions(&self, prefix: &str) -> Vec<String> {
    let mut fields = BTreeSet::from([
      "name".to_string(),
      "modified".to_string(),
      "created".to_string(),
      "size".to_string(),
      "format".to_string(),
      "dimensions".to_string(),
      "metadata".to_string(),
      "path".to_string(),
    ]);
    for item in &self.images {
      for entry in &item.metadata {
        fields.insert(entry.name.clone());
        fields.insert(format!("{}.{}", entry.group, entry.name));
      }
    }
    filter_completion_candidates(fields.iter(), prefix)
  }

  fn request_clear_cache(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.cache_clear_pending {
      self.set_message("clear-cache already running");
      return;
    }
    self.cache_clear_pending = true;
    let cache_dir = self.settings.cache_dir.clone();
    let tx = tx.clone();
    info!(cache_dir = %cache_dir.display(), "clear-cache requested");
    tokio::spawn(async move {
      let result = cache::clear_render_cache(&cache_dir)
        .await
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::CacheClear(CacheClearOutcome { result }));
    });
  }

  fn handle_scroll_down(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(1),
      ViewMode::Detail => self.focus_relative(1),
    }
  }

  fn handle_scroll_up(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(-1),
      ViewMode::Detail => self.focus_relative(-1),
    }
  }

  fn handle_mouse_click(&mut self, column: u16, row: u16) {
    if self.view != ViewMode::Browser || self.images.is_empty() {
      return;
    }
    let Some(layout) = &self.last_layout else {
      return;
    };
    let Some(viewport) = self.browser_viewport else {
      return;
    };
    if column < viewport.x
      || row < viewport.y
      || column >= viewport.x.saturating_add(viewport.width)
      || row >= viewport.y.saturating_add(viewport.height)
    {
      return;
    }

    let canvas_x = column.saturating_sub(viewport.x);
    let canvas_y = u32::from(row.saturating_sub(viewport.y)).saturating_add(self.browser_scroll);
    let target = layout.cards.iter().enumerate().find_map(|(index, card)| {
      let within_x = canvas_x >= card.x && canvas_x < card.x.saturating_add(card.width);
      let within_y = canvas_y >= card.y && canvas_y < card.y.saturating_add(u32::from(card.height));
      if within_x && within_y {
        Some(index)
      } else {
        None
      }
    });

    if let Some(index) = target.filter(|index| *index < self.images.len()) {
      self.focused = index;
      self.pending_keys.clear();
      self.hints.clear();
      self.ensure_focus_visible();
    }
  }

  fn ensure_focus_visible(&mut self) {
    if self.view != ViewMode::Browser {
      return;
    }
    let Some(layout) = &self.last_layout else {
      return;
    };
    let Some(card) = layout.cards.get(self.focused) else {
      return;
    };
    let top = card.y;
    let bottom = card.y.saturating_add(u32::from(card.height));
    let viewport_height = u32::from(self.browser_view_height.max(1));
    let visible_bottom = self.browser_scroll.saturating_add(viewport_height);
    if u32::from(card.height) >= viewport_height {
      if bottom <= self.browser_scroll || top >= visible_bottom {
        self.browser_scroll = top;
      }
      return;
    }

    if top < self.browser_scroll {
      self.browser_scroll = top;
    } else if bottom > visible_bottom {
      self.browser_scroll = bottom.saturating_sub(viewport_height);
    }
  }

  fn restore_focus(&mut self, path: Option<&Path>) {
    if let Some(path) = path {
      if let Some(idx) = self.images.iter().position(|item| item.path == path) {
        self.focused = idx;
        return;
      }
    }
    if self.images.is_empty() {
      self.focused = 0;
    } else {
      self.focused = self.focused.min(self.images.len() - 1);
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
  Ok(to)
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

#[derive(Debug, Clone)]
pub struct CommandCompletion {
  pub replace_start: usize,
  pub replace_end: usize,
  pub prefix: String,
  pub candidates: Vec<String>,
  pub append_space: bool,
  pub selected: usize,
}

impl CommandCompletion {
  fn new(
    replace_start: usize,
    replace_end: usize,
    prefix: impl Into<String>,
    candidates: Vec<String>,
    append_space: bool,
    selected: usize,
  ) -> Self {
    let selected = selected.min(candidates.len().saturating_sub(1));
    Self {
      replace_start,
      replace_end,
      prefix: prefix.into(),
      candidates,
      append_space,
      selected,
    }
  }

  pub fn selected_candidate(&self) -> Option<&String> {
    self.candidates.get(self.selected)
  }

  pub fn suggestion_suffix(&self) -> String {
    let Some(candidate) = self.selected_candidate() else {
      return String::new();
    };
    candidate
      .chars()
      .skip(self.prefix.chars().count())
      .collect()
  }
}

fn current_word_start(input: &str, cursor: usize) -> usize {
  input
    .get(..cursor.min(input.len()))
    .unwrap_or_default()
    .char_indices()
    .rev()
    .find(|(_, ch)| ch.is_whitespace())
    .map(|(idx, ch)| idx + ch.len_utf8())
    .unwrap_or(0)
}

fn previous_boundary(input: &str, cursor: usize) -> usize {
  let cursor = cursor.min(input.len());
  input
    .get(..cursor)
    .and_then(|prefix| prefix.char_indices().last().map(|(idx, _)| idx))
    .unwrap_or(0)
}

fn next_boundary(input: &str, cursor: usize) -> usize {
  let cursor = cursor.min(input.len());
  input
    .get(cursor..)
    .and_then(|suffix| suffix.char_indices().nth(1).map(|(idx, _)| cursor + idx))
    .unwrap_or(input.len())
}

fn filter_completion_candidates<I, S>(candidates: I, prefix: &str) -> Vec<String>
where
  I: IntoIterator<Item = S>,
  S: AsRef<str>,
{
  let normalized_prefix = prefix.trim_start_matches(':').to_ascii_lowercase();
  let mut out = candidates
    .into_iter()
    .filter_map(|candidate| {
      let candidate = candidate.as_ref();
      candidate
        .to_ascii_lowercase()
        .starts_with(&normalized_prefix)
        .then(|| candidate.to_string())
    })
    .collect::<Vec<_>>();
  out.sort();
  out.dedup();
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{AppConfig, KeymapConfig, Settings, ThemeConfig};
  use crate::layout::{BrowserLayout, CanvasRect};
  use crate::model::{ImageMetadataEntry, SortDirection, SortField};
  use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
  };
  use std::time::SystemTime;

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
    let nanos = SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    std::env::temp_dir().join(format!(
      "gallery-tui-{name}-{}-{nanos}.toml",
      std::process::id()
    ))
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

  #[test]
  fn q_quits_in_browser() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();

    app.handle_input(key('q'), &tx);

    assert!(app.should_quit());
    assert_eq!(app.view, ViewMode::Browser);
  }

  #[test]
  fn q_returns_from_detail_without_quitting() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.view = ViewMode::Detail;

    app.handle_input(key('q'), &tx);

    assert!(!app.should_quit());
    assert_eq!(app.view, ViewMode::Browser);
  }

  #[test]
  fn mouse_click_focuses_browser_card() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.images = vec![image("a.png"), image("b.png")];
    app.update_browser_layout(
      BrowserLayout {
        cards: vec![
          CanvasRect {
            x: 0,
            y: 0,
            width: 10,
            height: 6,
          },
          CanvasRect {
            x: 12,
            y: 0,
            width: 10,
            height: 6,
          },
        ],
        total_height: 6,
        columns: 2,
      },
      Rect::new(5, 3, 40, 10),
    );

    app.handle_input(click(18, 4), &tx);

    assert_eq!(app.focused, 1);
  }

  #[test]
  fn mouse_click_uses_browser_scroll_offset() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.images = vec![image("a.png"), image("b.png")];
    app.update_browser_layout(
      BrowserLayout {
        cards: vec![
          CanvasRect {
            x: 0,
            y: 0,
            width: 10,
            height: 6,
          },
          CanvasRect {
            x: 0,
            y: 8,
            width: 10,
            height: 6,
          },
        ],
        total_height: 14,
        columns: 1,
      },
      Rect::new(0, 0, 20, 6),
    );
    app.browser_scroll = 8;

    app.handle_input(click(4, 2), &tx);

    assert_eq!(app.focused, 1);
  }

  #[test]
  fn mouse_wheel_navigates_between_browser_rows() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.images = vec![
      image("a.png"),
      image("b.png"),
      image("c.png"),
      image("d.png"),
    ];
    app.focused = 1;
    app.update_browser_layout(
      BrowserLayout {
        cards: vec![
          CanvasRect {
            x: 0,
            y: 0,
            width: 10,
            height: 6,
          },
          CanvasRect {
            x: 12,
            y: 0,
            width: 10,
            height: 6,
          },
          CanvasRect {
            x: 0,
            y: 8,
            width: 10,
            height: 6,
          },
          CanvasRect {
            x: 12,
            y: 8,
            width: 10,
            height: 6,
          },
        ],
        total_height: 14,
        columns: 2,
      },
      Rect::new(0, 0, 30, 6),
    );

    app.handle_input(wheel(MouseEventKind::ScrollDown), &tx);
    assert_eq!(app.focused, 3);

    app.handle_input(wheel(MouseEventKind::ScrollUp), &tx);
    assert_eq!(app.focused, 1);
  }

  #[test]
  fn sort_command_updates_sort_spec() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();

    app.execute_command("sort created desc".to_string(), &tx);

    assert_eq!(app.sort_spec.field, SortField::Created);
    assert_eq!(app.sort_spec.direction, SortDirection::Desc);
  }

  #[test]
  fn sort_command_accepts_metadata_fields() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();

    app.execute_command("sort Exif.ExposureTime desc".to_string(), &tx);

    assert_eq!(
      app.sort_spec.field,
      SortField::Metadata("Exif.ExposureTime".to_string())
    );
    assert_eq!(app.sort_spec.direction, SortDirection::Desc);
  }

  #[test]
  fn keymap_sort_action_uses_sort_command_syntax() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();

    app.handle_action("sort name desc", &tx);

    assert_eq!(app.sort_spec.field, SortField::Name);
    assert_eq!(app.sort_spec.direction, SortDirection::Desc);
  }

  #[test]
  fn layout_command_updates_active_layout() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.settings.config_path = unique_config_path("layout-save");

    app.execute_command("layout grid 3 3".to_string(), &tx);

    let layout = app.settings.config.layout.effective();
    assert_eq!(layout.strategy, "grid");
    assert_eq!(layout.columns, 3);
    assert_eq!(layout.rows, 3);
    assert_eq!(app.message, "layout: grid 3x3 (saving)");

    let AsyncEvent::ConfigSave(outcome) = rx.try_recv().unwrap() else {
      panic!("expected config save outcome");
    };
    app.finish_config_save(outcome);
    assert_eq!(app.message, "layout saved: grid 3x3");
    let body = std::fs::read_to_string(&app.settings.config_path).unwrap();
    assert!(body.contains(r#"active = "grid""#));
    assert!(body.contains(r#""3""#));
  }

  #[test]
  fn keymap_layout_action_uses_layout_command_syntax() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();

    app.handle_action("layout-use list 8", &tx);

    let layout = app.settings.config.layout.effective();
    assert_eq!(layout.strategy, "list");
    assert_eq!(layout.items, 8);
    assert_eq!(app.message, "layout: list 8 (temporary)");
  }

  #[test]
  fn command_history_uses_up_and_down() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();

    app.prompt = Some(Prompt::command("sort name asc"));
    app.handle_prompt_key(prompt_key(KeyCode::Enter), &tx);
    app.prompt = Some(Prompt::command("draft"));

    app.handle_prompt_key(prompt_key(KeyCode::Up), &tx);
    assert_eq!(command_input(&app), "sort name asc");

    app.handle_prompt_key(prompt_key(KeyCode::Down), &tx);
    assert_eq!(command_input(&app), "draft");
  }

  #[test]
  fn command_completion_completes_layout_name() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.prompt = Some(Prompt::command("layout gr"));

    app.handle_prompt_key(prompt_key(KeyCode::Tab), &tx);
    assert_eq!(command_input(&app), "layout gr");

    app.handle_prompt_key(prompt_key(KeyCode::Enter), &tx);
    assert_eq!(command_input(&app), "layout grid ");
  }

  #[test]
  fn command_completion_uses_metadata_sort_fields() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    let mut item = image("a.png");
    item.metadata.push(ImageMetadataEntry {
      group: "Exif".to_string(),
      name: "ISO".to_string(),
      value: "100".to_string(),
    });
    app.images = vec![item];
    app.prompt = Some(Prompt::command("sort Exif.I"));

    app.handle_prompt_key(prompt_key(KeyCode::Tab), &tx);
    assert_eq!(command_input(&app), "sort Exif.I");

    app.handle_prompt_key(prompt_key(KeyCode::Enter), &tx);
    assert_eq!(command_input(&app), "sort Exif.ISO ");
  }

  #[test]
  fn command_completion_cycles_candidates_with_tab_and_backtab() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.prompt = Some(Prompt::command(String::new()));
    app.refresh_command_completion();

    let initial = app
      .command_completion
      .as_ref()
      .and_then(CommandCompletion::selected_candidate)
      .cloned();
    app.handle_prompt_key(prompt_key(KeyCode::Tab), &tx);
    let next = app
      .command_completion
      .as_ref()
      .and_then(CommandCompletion::selected_candidate)
      .cloned();
    app.handle_prompt_key(prompt_key(KeyCode::BackTab), &tx);
    let previous = app
      .command_completion
      .as_ref()
      .and_then(CommandCompletion::selected_candidate)
      .cloned();

    assert_ne!(initial, next);
    assert_eq!(initial, previous);
  }

  #[test]
  fn command_completion_exposes_inline_suggestion_suffix() {
    let mut app = test_app();
    app.prompt = Some(Prompt::command("layout gr"));
    app.refresh_command_completion();

    let suffix = app
      .command_completion
      .as_ref()
      .map(CommandCompletion::suggestion_suffix)
      .unwrap();

    assert_eq!(suffix, "id");
  }

  #[test]
  fn prompt_input_moves_cursor_and_edits_at_cursor() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.prompt = Some(Prompt::command("abc"));

    app.handle_prompt_key(prompt_key(KeyCode::Left), &tx);
    app.handle_prompt_key(prompt_key(KeyCode::Left), &tx);
    app.handle_prompt_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE), &tx);

    assert_eq!(command_input(&app), "aXbc");
    assert_eq!(app.command_buffer().unwrap().cursor, 2);

    app.handle_prompt_key(
      prompt_key_with(KeyCode::Char('a'), KeyModifiers::CONTROL),
      &tx,
    );
    app.handle_prompt_key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE), &tx);
    assert_eq!(command_input(&app), "YaXbc");

    app.handle_prompt_key(
      prompt_key_with(KeyCode::Char('e'), KeyModifiers::CONTROL),
      &tx,
    );
    app.handle_prompt_key(
      prompt_key_with(KeyCode::Char('u'), KeyModifiers::CONTROL),
      &tx,
    );
    assert_eq!(command_input(&app), "");
  }

  #[test]
  fn prompt_input_requests_external_editor() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.prompt = Some(Prompt::command("draft"));

    app.handle_prompt_key(
      prompt_key_with(KeyCode::Char('g'), KeyModifiers::CONTROL),
      &tx,
    );

    let Some(EditorRequest::Prompt { input }) = app.take_editor_request() else {
      panic!("expected prompt editor request");
    };
    assert_eq!(input, "draft");
  }

  #[test]
  fn rename_prompt_places_cursor_before_extension() {
    let mut app = test_app();
    app.images = vec![image("photo.final.jpg")];

    app.start_rename();

    let buffer = rename_buffer(&app);
    assert_eq!(buffer.input, "photo.final.jpg");
    assert_eq!(buffer.cursor, "photo.final".len());
  }

  #[test]
  fn metadata_editor_can_request_filename_change() {
    let mut app = test_app();
    let path = PathBuf::from("/tmp/gallery-tui-test-root/old.jpg");

    app.finish_metadata_editor_input(
      path.clone(),
      Vec::new(),
      Ok("[file]\nname = \"new.jpg\"\n\n[tags]\n".to_string()),
    );

    let Some(ConfirmDialog::MetadataWrite {
      path: actual_path,
      edit,
    }) = app.confirm
    else {
      panic!("expected metadata confirmation");
    };
    assert_eq!(actual_path, path);
    let change = edit.file_name.unwrap();
    assert_eq!(change.old_value, "old.jpg");
    assert_eq!(change.new_value, "new.jpg");
    assert!(edit.tags.is_empty());
  }

  #[test]
  fn confirm_enter_cancels_by_default() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = test_app();
    app.confirm = Some(ConfirmDialog::MetadataWrite {
      path: PathBuf::from("/tmp/gallery-tui-test-root/old.jpg"),
      edit: MetadataEdit::default(),
    });

    app.handle_input(
      Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
      &tx,
    );

    assert!(app.confirm.is_none());
    assert_eq!(app.message, "cancelled");
  }

  #[test]
  fn tall_focused_card_does_not_oscillate_scroll() {
    let mut app = test_app();
    app.images = vec![image("a.png")];
    app.focused = 0;
    app.update_browser_layout(
      BrowserLayout {
        cards: vec![CanvasRect {
          x: 0,
          y: 0,
          width: 10,
          height: 40,
        }],
        total_height: 40,
        columns: 1,
      },
      Rect::new(0, 0, 20, 10),
    );
    app.browser_scroll = 30;

    app.ensure_focus_visible();
    assert_eq!(app.browser_scroll, 30);

    app.ensure_focus_visible();
    assert_eq!(app.browser_scroll, 30);
  }
}
