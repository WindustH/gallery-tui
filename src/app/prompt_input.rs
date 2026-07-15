use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::event::AsyncEvent;
use framework_tui::{KeyContext, MatchResult, key_event_to_token};

use super::{
  App, COMMAND_NAMES, CommandCompletion, EditorRequest, Prompt, PromptBuffer, current_word_start,
  filter_completion_candidates,
};

impl App {
  pub(super) fn handle_prompt_paste(&mut self, value: &str) {
    if let Some(prompt) = self.prompt.as_mut() {
      prompt.buffer_mut().insert_str(value);
    }
    self.reset_command_history_cursor();
    self.refresh_command_completion();
  }

  pub(super) fn handle_prompt_key(
    &mut self,
    key: KeyEvent,
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
      KeyCode::Char(ch) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
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
          self.command_state.clear_completion();
        }
      }
      other => self.set_message(format!("unknown input action: {other}")),
    }
  }

  fn cancel_prompt(&mut self) {
    self.prompt = None;
    self.command_state.reset_prompt_state();
    self.set_message("cancelled");
  }

  fn submit_prompt(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.complete_selected_command_candidate() {
      return;
    }
    let prompt = self.prompt.take();
    self.command_state.clear_completion();
    match prompt {
      Some(Prompt::Text { buffer, .. }) => self.request_rename(buffer.input, tx),
      Some(Prompt::Command { buffer }) => self.submit_command(buffer.input, tx),
      None => {}
    }
  }

  pub(super) fn command_buffer(&self) -> Option<&PromptBuffer> {
    match self.prompt.as_ref()? {
      Prompt::Command { buffer } => Some(buffer),
      Prompt::Text { .. } => None,
    }
  }

  fn command_buffer_mut(&mut self) -> Option<&mut PromptBuffer> {
    match self.prompt.as_mut()? {
      Prompt::Command { buffer } => Some(buffer),
      Prompt::Text { .. } => None,
    }
  }

  pub(super) fn reset_command_history_cursor(&mut self) {
    if self.command_buffer().is_some() {
      self.command_state.reset_history_cursor();
    }
  }

  fn command_history_previous(&mut self) {
    let mut command_state = std::mem::take(&mut self.command_state);
    if let Some(buffer) = self.command_buffer_mut() {
      command_state.history_previous(buffer);
    }
    self.command_state = command_state;
    self.refresh_command_completion();
  }

  fn command_history_next(&mut self) {
    let mut command_state = std::mem::take(&mut self.command_state);
    if let Some(buffer) = self.command_buffer_mut() {
      command_state.history_next(buffer);
    }
    self.command_state = command_state;
    self.refresh_command_completion();
  }

  pub(super) fn refresh_command_completion(&mut self) {
    let Some(buffer) = self.command_buffer() else {
      self.command_state.clear_completion();
      return;
    };
    let input = buffer.input.clone();
    let cursor = buffer.cursor;

    let completion = self.command_completion_for(&input, cursor);
    self
      .command_state
      .set_completion_preserving_selection(completion);
  }

  fn select_next_completion(&mut self) {
    self.refresh_command_completion();
    self.command_state.select_next_completion();
  }

  fn select_previous_completion(&mut self) {
    self.refresh_command_completion();
    self.command_state.select_previous_completion();
  }

  fn complete_selected_command_candidate(&mut self) -> bool {
    self.refresh_command_completion();
    let mut command_state = std::mem::take(&mut self.command_state);
    let changed = if let Some(buffer) = self.command_buffer_mut() {
      command_state.apply_completion(buffer)
    } else {
      false
    };
    self.command_state = command_state;
    if changed {
      self.refresh_command_completion();
    }
    changed
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
}
