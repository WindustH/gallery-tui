use crossterm::event::{Event, MouseButton, MouseEventKind};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
  event::{AsyncEvent, MetadataWriteOutcome},
  keymap::{KeyContext, MatchResult, key_event_to_token},
  metadata,
};

use super::{
  App, ConfirmDialog, EditorRequest, Prompt, ViewMode, action_is_layout_command,
  action_is_sort_command, rename_cursor_position, rename_file_no_replace, validate_new_file_name,
};

impl App {
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
              rename_file_no_replace(&path, &to)?;
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
          .match_sequence(self.key_context(), std::slice::from_ref(&token))
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
          | "clear_selection"
          | "copy_paths"
      ),
      ViewMode::Detail => matches!(
        action,
        "back" | "move_left" | "move_down" | "move_up" | "move_right" | "edit_metadata"
      ),
    }
  }

  pub(super) fn handle_action(&mut self, action: &str, tx: &mpsc::UnboundedSender<AsyncEvent>) {
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
      "clear_selection" => self.clear_selection(),
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

  pub(super) fn start_rename(&mut self) {
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
}
