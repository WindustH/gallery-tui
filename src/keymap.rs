use std::collections::{BTreeMap, BTreeSet};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config::{KeymapConfig, KeymapEntry, KeymapOn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
  Browser,
  Detail,
  Input,
}

#[derive(Debug, Clone)]
pub struct KeyBindings {
  browser: Vec<Binding>,
  detail: Vec<Binding>,
  input: Vec<Binding>,
  global: Vec<Binding>,
}

#[derive(Debug, Clone)]
struct Binding {
  action: String,
  sequence: Vec<String>,
  desc: String,
}

#[derive(Debug, Clone)]
pub enum MatchResult {
  None,
  Prefix(Vec<KeyHint>),
  Action(String),
}

#[derive(Debug, Clone)]
pub struct KeyHint {
  pub key: String,
  pub label: String,
}

impl KeyBindings {
  pub fn from_config(config: &KeymapConfig) -> Self {
    Self {
      browser: parse_entries(&config.browser.keymap),
      detail: parse_entries(&config.detail.keymap),
      input: parse_entries(&config.input.keymap),
      global: parse_entries(&config.global.keymap),
    }
  }

  pub fn match_sequence(&self, context: KeyContext, sequence: &[String]) -> MatchResult {
    match context {
      KeyContext::Browser => match_bindings(
        [&self.browser[..], &self.global[..]].into_iter().flatten(),
        sequence,
      ),
      KeyContext::Detail => match_bindings(
        [&self.detail[..], &self.global[..]].into_iter().flatten(),
        sequence,
      ),
      KeyContext::Input => match_bindings(self.input.iter(), sequence),
    }
  }
}

fn parse_entries(entries: &[KeymapEntry]) -> Vec<Binding> {
  entries
    .iter()
    .filter_map(|entry| {
      let sequence = parse_on(&entry.on);
      if sequence.is_empty() {
        return None;
      }
      Some(Binding {
        action: entry.run.clone(),
        sequence,
        desc: entry.desc.clone(),
      })
    })
    .collect()
}

fn match_bindings<'a>(
  bindings: impl Iterator<Item = &'a Binding>,
  sequence: &[String],
) -> MatchResult {
  let mut exact = None;
  let mut next = BTreeMap::<String, BTreeSet<String>>::new();
  for binding in bindings {
    if !binding.sequence.starts_with(sequence) {
      continue;
    }
    if binding.sequence.len() == sequence.len() {
      exact = Some(binding.action.clone());
    } else if let Some(key) = binding.sequence.get(sequence.len()) {
      next
        .entry(key.clone())
        .or_default()
        .insert(binding.desc.clone());
    }
  }

  if let Some(action) = exact
    && next.is_empty()
  {
    return MatchResult::Action(action);
  }

  if !next.is_empty() {
    let hints = next
      .into_iter()
      .map(|(key, labels)| KeyHint {
        key,
        label: labels.into_iter().collect::<Vec<_>>().join(", "),
      })
      .collect();
    return MatchResult::Prefix(hints);
  }

  MatchResult::None
}

fn parse_on(on: &KeymapOn) -> Vec<String> {
  match on {
    KeymapOn::One(value) => parse_key(value).into_iter().collect(),
    KeymapOn::Many(values) => values.iter().filter_map(|value| parse_key(value)).collect(),
  }
}

fn parse_key(value: &str) -> Option<String> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return None;
  }

  let canonical = if let Some(inner) = trimmed.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
    match inner.to_ascii_lowercase().as_str() {
      "space" => "space".to_string(),
      "enter" | "return" => "enter".to_string(),
      "esc" | "escape" => "esc".to_string(),
      "tab" => "tab".to_string(),
      "backtab" | "s-tab" => "backtab".to_string(),
      "left" => "left".to_string(),
      "right" => "right".to_string(),
      "up" => "up".to_string(),
      "down" => "down".to_string(),
      "home" => "home".to_string(),
      "end" => "end".to_string(),
      "pageup" | "page-up" | "pgup" => "pgup".to_string(),
      "pagedown" | "page-down" | "pgdn" => "pgdn".to_string(),
      "c-[" => "ctrl-[".to_string(),
      key if key.starts_with("c-") => format!("ctrl-{}", &inner[2..].to_ascii_lowercase()),
      key if key.starts_with("a-") => format!("alt-{}", &inner[2..]),
      key if key.starts_with('f') => key.to_string(),
      _ => inner.to_string(),
    }
  } else {
    match trimmed.to_ascii_lowercase().as_str() {
      "pageup" => "pgup".to_string(),
      "pagedown" => "pgdn".to_string(),
      other if other.starts_with("ctrl-") => other.to_string(),
      other if other.starts_with("alt-") => other.to_string(),
      _ => trimmed.to_string(),
    }
  };

  Some(canonical)
}

pub fn key_event_to_token(event: KeyEvent) -> Option<String> {
  if event.kind != KeyEventKind::Press {
    return None;
  }

  let base = match event.code {
    KeyCode::Backspace => "backspace".to_string(),
    KeyCode::Enter => "enter".to_string(),
    KeyCode::Left => "left".to_string(),
    KeyCode::Right => "right".to_string(),
    KeyCode::Up => "up".to_string(),
    KeyCode::Down => "down".to_string(),
    KeyCode::Home => "home".to_string(),
    KeyCode::End => "end".to_string(),
    KeyCode::PageUp => "pgup".to_string(),
    KeyCode::PageDown => "pgdn".to_string(),
    KeyCode::Tab => "tab".to_string(),
    KeyCode::BackTab => "backtab".to_string(),
    KeyCode::Delete => "delete".to_string(),
    KeyCode::Insert => "insert".to_string(),
    KeyCode::Esc => "esc".to_string(),
    KeyCode::Char(' ') => "space".to_string(),
    KeyCode::Char(ch) => ch.to_string(),
    KeyCode::F(number) => format!("f{number}"),
    _ => return None,
  };

  if event.modifiers.contains(KeyModifiers::CONTROL) {
    Some(format!("ctrl-{}", base.to_ascii_lowercase()))
  } else if event.modifiers.contains(KeyModifiers::ALT) {
    Some(format!("alt-{base}"))
  } else {
    Some(base)
  }
}
