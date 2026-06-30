use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::{
  event::AsyncEvent,
  keymap::{KeyContext, MatchResult, key_event_to_token},
};

use super::{
  App, COMMAND_NAMES, CommandCompletion, EditorRequest, Prompt, PromptBuffer, current_word_start,
  filter_completion_candidates,
};

impl App {
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

  pub(super) fn command_buffer(&self) -> Option<&PromptBuffer> {
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

  pub(super) fn reset_command_history_cursor(&mut self) {
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

  pub(super) fn refresh_command_completion(&mut self) {
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
    if let Some(previous) = previous
      && let Some(candidate) = previous.selected_candidate()
      && let Some(index) = completion
        .candidates
        .iter()
        .position(|value| value == candidate)
    {
      completion.selected = index;
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
}
