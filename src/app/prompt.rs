use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

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

#[derive(Debug, Clone)]
pub struct CommandCompletion {
  pub replace_start: usize,
  pub replace_end: usize,
  pub prefix: String,
  pub candidates: Vec<String>,
  pub append_space: bool,
  pub selected: usize,
}

impl PromptBuffer {
  pub(super) fn new(input: impl Into<String>) -> Self {
    let input = input.into();
    let cursor = input.len();
    Self { input, cursor }
  }

  pub(super) fn set_input(&mut self, input: String) {
    self.input = input;
    self.cursor = self.input.len();
  }

  pub(super) fn insert_char(&mut self, ch: char) {
    self.input.insert(self.cursor, ch);
    self.cursor += ch.len_utf8();
  }

  pub(super) fn insert_str(&mut self, value: &str) {
    let value = sanitize_inline_input(value);
    if value.is_empty() {
      return;
    }
    self.input.insert_str(self.cursor, &value);
    self.cursor += value.len();
  }

  pub(super) fn backspace(&mut self) {
    if self.cursor == 0 {
      return;
    }
    let previous = previous_boundary(&self.input, self.cursor);
    self.input.drain(previous..self.cursor);
    self.cursor = previous;
  }

  pub(super) fn delete(&mut self) {
    if self.cursor >= self.input.len() {
      return;
    }
    let next = next_boundary(&self.input, self.cursor);
    self.input.drain(self.cursor..next);
  }

  pub(super) fn move_left(&mut self) {
    self.cursor = previous_boundary(&self.input, self.cursor);
  }

  pub(super) fn move_right(&mut self) {
    self.cursor = next_boundary(&self.input, self.cursor);
  }

  pub(super) fn move_start(&mut self) {
    self.cursor = 0;
  }

  pub(super) fn move_end(&mut self) {
    self.cursor = self.input.len();
  }

  pub(super) fn kill_before_cursor(&mut self) {
    self.input.drain(..self.cursor);
    self.cursor = 0;
  }

  pub(super) fn kill_after_cursor(&mut self) {
    self.input.truncate(self.cursor);
  }

  pub fn cursor_columns(&self) -> usize {
    UnicodeWidthStr::width(&self.input[..self.cursor])
  }
}

impl Prompt {
  pub(super) fn rename(input: impl Into<String>) -> Self {
    Self::Rename {
      buffer: PromptBuffer::new(input),
    }
  }

  pub(super) fn command(input: impl Into<String>) -> Self {
    Self::Command {
      buffer: PromptBuffer::new(input),
    }
  }

  pub fn buffer(&self) -> &PromptBuffer {
    match self {
      Prompt::Rename { buffer } | Prompt::Command { buffer } => buffer,
    }
  }

  pub(super) fn buffer_mut(&mut self) -> &mut PromptBuffer {
    match self {
      Prompt::Rename { buffer } | Prompt::Command { buffer } => buffer,
    }
  }
}

impl EditorRequest {
  pub fn initial_text(&self) -> &str {
    match self {
      EditorRequest::Prompt { input } => input,
      EditorRequest::Metadata { draft, .. } => draft,
    }
  }
}

impl CommandCompletion {
  pub(super) fn new(
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

pub(super) fn current_word_start(input: &str, cursor: usize) -> usize {
  input
    .get(..cursor.min(input.len()))
    .unwrap_or_default()
    .char_indices()
    .rev()
    .find(|(_, ch)| ch.is_whitespace())
    .map(|(idx, ch)| idx + ch.len_utf8())
    .unwrap_or(0)
}

pub(super) fn filter_completion_candidates<I, S>(candidates: I, prefix: &str) -> Vec<String>
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

fn sanitize_inline_input(value: &str) -> String {
  value
    .chars()
    .map(|ch| match ch {
      '\r' | '\n' => ' ',
      ch => ch,
    })
    .collect()
}
