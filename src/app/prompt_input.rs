use std::collections::BTreeSet;

use crossterm::event::KeyEvent;
use tokio::sync::mpsc;

use crate::event::AsyncEvent;
use framework_tui::{PromptInputResult, handle_prompt_key, handle_prompt_paste};

use super::{
  App, COMMAND_NAMES, CommandCompletion, EditorRequest, NO_ARG_COMMANDS, Prompt, PromptBuffer,
  current_word_start, filter_completion_candidates,
};

impl App {
  pub(super) fn handle_prompt_paste(&mut self, value: &str) {
    if let Some(prompt) = self.prompt.as_mut() {
      let result = handle_prompt_paste(prompt, &mut self.command_state, value);
      self.handle_prompt_input_result(result, None);
    }
  }

  pub(super) fn handle_prompt_key(
    &mut self,
    key: KeyEvent,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
  ) {
    let result = if let Some(prompt) = self.prompt.as_mut() {
      handle_prompt_key(prompt, &mut self.command_state, &self.keymap, key)
    } else {
      PromptInputResult::Unhandled
    };
    self.handle_prompt_input_result(result, Some(tx));
  }

  fn cancel_prompt(&mut self) {
    self.prompt = None;
    self.command_state.reset_prompt_state();
    self.set_message("cancelled");
  }

  fn submit_prompt(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let prompt = self.prompt.take();
    self.command_state.clear_completion();
    match prompt {
      Some(Prompt::Text { buffer, .. }) => self.request_rename(buffer.input, tx),
      Some(Prompt::Command { buffer }) => self.submit_command(buffer.input, tx),
      None => {}
    }
  }

  fn handle_prompt_input_result(
    &mut self,
    result: PromptInputResult,
    tx: Option<&mpsc::UnboundedSender<AsyncEvent>>,
  ) {
    match result {
      PromptInputResult::Unhandled => {}
      PromptInputResult::Changed => self.refresh_command_completion(),
      PromptInputResult::Cancel => self.cancel_prompt(),
      PromptInputResult::Submit => {
        if let Some(tx) = tx {
          self.submit_prompt(tx);
        }
      }
      PromptInputResult::EditInEditor { input } => {
        self.editor_request = Some(EditorRequest::Prompt { input });
        self.command_state.clear_completion();
      }
      PromptInputResult::UnknownAction(action) if action == "help" => {
        self.key_help = true;
        self.set_message("key bindings");
      }
      PromptInputResult::UnknownAction(action) => {
        self.set_message(format!("unknown input action: {action}"));
      }
    }
  }

  pub(super) fn command_buffer(&self) -> Option<&PromptBuffer> {
    match self.prompt.as_ref()? {
      Prompt::Command { buffer } => Some(buffer),
      Prompt::Text { .. } => None,
    }
  }

  pub(super) fn reset_command_history_cursor(&mut self) {
    if self.command_buffer().is_some() {
      self.command_state.reset_history_cursor();
    }
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

  fn command_completion_for(&mut self, input: &str, cursor: usize) -> Option<CommandCompletion> {
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
      // A fully typed command without arguments is complete: no trailing
      // space, so Enter runs it instead of only inserting the space.
      let complete = NO_ARG_COMMANDS.contains(&prefix.trim_start_matches(':'));
      return Some(CommandCompletion::new(
        word_start,
        cursor,
        prefix,
        filter_completion_candidates(COMMAND_NAMES.iter().copied(), prefix),
        !complete,
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
        // Layout arguments are optional, so a fully typed name can run as is.
        let presets = &self.settings.config.layout.presets;
        Some(CommandCompletion::new(
          replace_start,
          cursor,
          prefix,
          filter_completion_candidates(presets.keys(), prefix),
          !presets.contains_key(prefix),
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

  fn sort_field_completions(&mut self, prefix: &str) -> Vec<String> {
    let images = &self.images;
    let fields = self.metadata_fields.get_or_insert_with(|| {
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
      for entry in images.iter().flat_map(|item| &item.metadata) {
        if !fields.contains(&entry.name) {
          fields.insert(entry.name.clone());
        }
        fields.insert(format!("{}.{}", entry.group, entry.name));
      }
      fields.into_iter().collect()
    });
    filter_completion_candidates(fields.iter(), prefix)
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use super::*;
  use crate::config::{AppConfig, KeymapConfig, Settings, ThemeConfig};

  fn app() -> App {
    let settings = Settings {
      config: AppConfig::default(),
      keymap: KeymapConfig::default(),
      theme: ThemeConfig::default(),
      config_path: PathBuf::new(),
      cache_dir: PathBuf::new(),
    };
    App::new(PathBuf::from("/"), settings, Vec::new())
  }

  /// The prompt input after Enter applies the completion, or `None` when
  /// Enter submits the command as typed.
  fn after_enter(app: &mut App, input: &str) -> Option<String> {
    let completion = app.command_completion_for(input, input.len())?;
    let mut buffer = PromptBuffer::new(input);
    completion.apply_to(&mut buffer).then_some(buffer.input)
  }

  #[test]
  fn enter_runs_fully_typed_commands_that_need_no_arguments() {
    let mut app = app();
    assert_eq!(after_enter(&mut app, "refresh"), None);
    assert_eq!(after_enter(&mut app, "clear-cache"), None);
    assert_eq!(after_enter(&mut app, "layout-use list"), None);
    assert_eq!(after_enter(&mut app, "sort name asc"), None);
  }

  #[test]
  fn enter_completes_partial_words() {
    let mut app = app();
    assert_eq!(after_enter(&mut app, "ref").as_deref(), Some("refresh "));
    assert_eq!(after_enter(&mut app, "layout").as_deref(), Some("layout "));
    assert_eq!(
      after_enter(&mut app, "layout mas").as_deref(),
      Some("layout masonry ")
    );
    assert_eq!(
      after_enter(&mut app, "sort na").as_deref(),
      Some("sort name ")
    );
  }
}
