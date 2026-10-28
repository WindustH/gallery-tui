use framework_tui::theme::parse_color;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use super::file::ConfigFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
  pub foreground: String,
  pub background: String,
  pub muted: String,
  pub accent: String,
  pub border: String,
  pub focused_border: String,
  pub selected_border: String,
  #[serde(default = "default_selected_foreground")]
  pub selected_foreground: String,
  pub selected_background: String,
  #[serde(default = "default_hover_foreground")]
  pub hover_foreground: String,
  #[serde(default = "default_hover_background")]
  pub hover_background: String,
  #[serde(default = "default_hover_selected_foreground")]
  pub hover_selected_foreground: String,
  #[serde(default = "default_hover_selected_background")]
  pub hover_selected_background: String,
  pub error: String,
  pub which_key_columns: u16,
  pub which_key_background: String,
  pub which_key_foreground: String,
  pub which_key_key: String,
  pub which_key_rest: String,
  pub which_key_description: String,
  pub which_key_separator: String,
  pub which_key_separator_color: String,
}

impl Default for ThemeConfig {
  fn default() -> Self {
    Self {
      foreground: "white".to_string(),
      background: "reset".to_string(),
      muted: "dark_gray".to_string(),
      accent: "cyan".to_string(),
      border: "dark_gray".to_string(),
      focused_border: "yellow".to_string(),
      selected_border: "green".to_string(),
      selected_foreground: default_selected_foreground(),
      selected_background: "white".to_string(),
      hover_foreground: default_hover_foreground(),
      hover_background: default_hover_background(),
      hover_selected_foreground: default_hover_selected_foreground(),
      hover_selected_background: default_hover_selected_background(),
      error: "red".to_string(),
      which_key_columns: 3,
      which_key_background: "reset".to_string(),
      which_key_foreground: "white".to_string(),
      which_key_key: "light_cyan".to_string(),
      which_key_rest: "dark_gray".to_string(),
      which_key_description: "light_magenta".to_string(),
      which_key_separator: " -> ".to_string(),
      which_key_separator_color: "dark_gray".to_string(),
    }
  }
}

impl ThemeConfig {
  pub fn color(&self, value: &str) -> Color {
    parse_color(value)
  }

  pub fn foreground_color(&self, value: &str, background: &str) -> Color {
    if value.trim().eq_ignore_ascii_case("auto") {
      auto_foreground_for(background)
    } else {
      parse_color(value)
    }
  }
}

impl ConfigFile for ThemeConfig {}

fn default_selected_foreground() -> String {
  "auto".to_string()
}

fn default_hover_foreground() -> String {
  "black".to_string()
}

fn default_hover_background() -> String {
  "cyan".to_string()
}

fn default_hover_selected_foreground() -> String {
  "black".to_string()
}

fn default_hover_selected_background() -> String {
  "cyan".to_string()
}

fn auto_foreground_for(background: &str) -> Color {
  color_luminance(parse_color(background))
    .map(|luminance| {
      if luminance >= 0.5 {
        Color::Black
      } else {
        Color::White
      }
    })
    .unwrap_or(Color::Reset)
}

fn color_luminance(color: Color) -> Option<f32> {
  match color {
    Color::Reset => None,
    Color::Black => Some(0.0),
    Color::Red => Some(rgb_luminance(128, 0, 0)),
    Color::Green => Some(rgb_luminance(0, 128, 0)),
    Color::Yellow => Some(rgb_luminance(128, 128, 0)),
    Color::Blue => Some(rgb_luminance(0, 0, 128)),
    Color::Magenta => Some(rgb_luminance(128, 0, 128)),
    Color::Cyan => Some(rgb_luminance(0, 128, 128)),
    Color::Gray => Some(rgb_luminance(192, 192, 192)),
    Color::DarkGray => Some(rgb_luminance(128, 128, 128)),
    Color::LightRed => Some(rgb_luminance(255, 0, 0)),
    Color::LightGreen => Some(rgb_luminance(0, 255, 0)),
    Color::LightYellow => Some(rgb_luminance(255, 255, 0)),
    Color::LightBlue => Some(rgb_luminance(0, 0, 255)),
    Color::LightMagenta => Some(rgb_luminance(255, 0, 255)),
    Color::LightCyan => Some(rgb_luminance(0, 255, 255)),
    Color::White => Some(1.0),
    Color::Indexed(index) => indexed_color_luminance(index),
    Color::Rgb(r, g, b) => Some(rgb_luminance(r, g, b)),
  }
}

fn indexed_color_luminance(index: u8) -> Option<f32> {
  const BASIC: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
  ];
  if let Some((r, g, b)) = BASIC.get(index as usize).copied() {
    return Some(rgb_luminance(r, g, b));
  }
  if (16..=231).contains(&index) {
    let cube = index - 16;
    let r = color_cube_component(cube / 36);
    let g = color_cube_component((cube % 36) / 6);
    let b = color_cube_component(cube % 6);
    return Some(rgb_luminance(r, g, b));
  }
  if (232..=255).contains(&index) {
    let gray = 8 + (index - 232) * 10;
    return Some(rgb_luminance(gray, gray, gray));
  }
  None
}

fn color_cube_component(value: u8) -> u8 {
  match value {
    0 => 0,
    value => 55 + value * 40,
  }
}

fn rgb_luminance(r: u8, g: u8, b: u8) -> f32 {
  (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}
