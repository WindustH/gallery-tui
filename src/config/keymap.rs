use anyhow::Result;
use framework_tui::keymap::{
  InputKeymapOptions, KeyBindings, KeymapSection, default_input_keymap, format_keymap_sections, key,
};
use serde::{Deserialize, Serialize};

use super::file::ConfigFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapConfig {
  pub browser: KeymapSection,
  pub detail: KeymapSection,
  pub input: KeymapSection,
  pub global: KeymapSection,
}

impl Default for KeymapConfig {
  fn default() -> Self {
    Self {
      browser: KeymapSection {
        keymap: vec![
          key("q", "quit", "Quit gallery-tui"),
          key("ctrl-c", "quit", "Quit gallery-tui"),
          key("enter", "open", "Open detail page"),
          key("h", "move_left", "Move focus left"),
          key("left", "move_left", "Move focus left"),
          key("j", "move_down", "Move focus down"),
          key("down", "move_down", "Move focus down"),
          key("k", "move_up", "Move focus up"),
          key("up", "move_up", "Move focus up"),
          key("l", "move_right", "Move focus right"),
          key("right", "move_right", "Move focus right"),
          key("pgup", "page_up", "Move one page up"),
          key("pgdn", "page_down", "Move one page down"),
          key("pagedown", "page_down", "Move one page down"),
          key("home", "home", "Go to first image"),
          key(["g", "g"], "home", "Go to first image"),
          key("end", "end", "Go to last image"),
          key("G", "end", "Go to last image"),
          key("space", "toggle_select", "Toggle selection"),
          key("esc", "clear_selection", "Clear selection"),
          key(["c", "p"], "copy_paths", "Output selected paths"),
          key(["s", "n"], "sort name asc", "Sort by name ascending"),
          key(["s", "N"], "sort name desc", "Sort by name descending"),
          key(
            ["s", "m"],
            "sort modified asc",
            "Sort by modified time ascending",
          ),
          key(
            ["s", "M"],
            "sort modified desc",
            "Sort by modified time descending",
          ),
          key(["s", "z"], "sort size asc", "Sort by size ascending"),
          key(["s", "S"], "sort size desc", "Sort by size descending"),
        ],
      },
      detail: KeymapSection {
        keymap: vec![
          key("q", "back", "Return to browser"),
          key("h", "move_left", "Show image page"),
          key("left", "move_left", "Show image page"),
          key("l", "move_right", "Show metadata page"),
          key("right", "move_right", "Show metadata page"),
          key("j", "move_down", "Next image"),
          key("down", "move_down", "Next image"),
          key("k", "move_up", "Previous image"),
          key("up", "move_up", "Previous image"),
          key(["g", "g"], "home", "Go to first image"),
          key("G", "end", "Go to last image"),
          key("e", "edit_metadata", "Edit metadata in $EDITOR"),
        ],
      },
      input: default_input_keymap(&InputKeymapOptions {
        edit_in_editor: true,
        ..InputKeymapOptions::default()
      }),
      global: KeymapSection {
        keymap: vec![
          key("f1", "help", "Show key bindings"),
          key("r", "rename", "Rename image"),
          key(":", "command", "Enter command"),
        ],
      },
    }
  }
}

impl KeymapConfig {
  pub fn bindings(&self) -> KeyBindings {
    KeyBindings::from_sections(
      self.browser.binding_configs(),
      self.detail.binding_configs(),
      self.input.binding_configs(),
      self.global.binding_configs(),
    )
  }
}

impl ConfigFile for KeymapConfig {
  /// Add default bindings for actions the file does not bind at all.
  fn normalize(&mut self) {
    let default = KeymapConfig::default();
    self.browser.append_missing_actions(&default.browser);
    self.detail.append_missing_actions(&default.detail);
    self.input.append_missing_actions(&default.input);
    self.global.append_missing_actions(&default.global);
  }

  fn to_toml(&self) -> Result<String> {
    Ok(format_keymap_toml(self))
  }
}

fn format_keymap_toml(config: &KeymapConfig) -> String {
  format_keymap_sections([
    ("browser", &config.browser),
    ("detail", &config.detail),
    ("input", &config.input),
    ("global", &config.global),
  ])
}
