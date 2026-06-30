use std::{
  collections::BTreeMap,
  fmt::Write as FmtWrite,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::model::{SortDirection, SortField, SortSpec};

#[derive(Debug, Clone)]
pub struct Settings {
  pub config: AppConfig,
  pub keymap: KeymapConfig,
  pub theme: ThemeConfig,
  pub config_path: PathBuf,
  pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
  pub recursive: bool,
  pub initial_sort: String,
  pub supported_extensions: Vec<String>,
  pub layout: LayoutConfig,
  pub render: RenderConfig,
  pub behavior: BehaviorConfig,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      recursive: false,
      initial_sort: "name_asc".to_string(),
      supported_extensions: [
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "qoi", "ico", "pnm",
        "tga",
      ]
      .into_iter()
      .map(str::to_string)
      .collect(),
      layout: LayoutConfig::default(),
      render: RenderConfig::default(),
      behavior: BehaviorConfig::default(),
    }
  }
}

impl AppConfig {
  pub fn initial_sort_spec(&self) -> SortSpec {
    SortSpec::parse(&self.initial_sort).unwrap_or_default()
  }

  fn normalize_defaults(&mut self) {
    self.layout.normalize_defaults();
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
  #[serde(default = "default_layout_active")]
  pub active: String,
  #[serde(default = "default_layout_active_args")]
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub active_args: Vec<String>,
  #[serde(default = "default_gap_x")]
  pub gap_x: u16,
  #[serde(default = "default_gap_y")]
  pub gap_y: u16,
  #[serde(default = "default_card_style")]
  pub card_style: String,
  #[serde(default = "default_show_filename")]
  pub show_filename: bool,
  #[serde(default = "default_filename_position")]
  pub filename_position: String,
  #[serde(default = "default_image_alignment")]
  pub image_alignment: String,
  #[serde(default = "default_image_ratio")]
  pub image_ratio: f32,
  #[serde(default = "default_label_lines")]
  pub label_lines: u16,
  #[serde(default = "default_show_border")]
  pub show_border: bool,
  #[serde(default = "default_padding")]
  pub padding: u16,
  #[serde(default = "default_layout_presets")]
  pub presets: BTreeMap<String, LayoutPresetConfig>,
}

impl Default for LayoutConfig {
  fn default() -> Self {
    Self {
      active: default_layout_active(),
      active_args: default_layout_active_args(),
      gap_x: default_gap_x(),
      gap_y: default_gap_y(),
      card_style: default_card_style(),
      show_filename: default_show_filename(),
      filename_position: default_filename_position(),
      image_alignment: default_image_alignment(),
      image_ratio: default_image_ratio(),
      label_lines: default_label_lines(),
      show_border: default_show_border(),
      padding: default_padding(),
      presets: default_layout_presets(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutPresetConfig {
  #[serde(default = "default_layout_strategy")]
  pub strategy: String,
  pub params: Vec<String>,
  pub columns: u16,
  pub rows: u16,
  pub items: u16,
  #[serde(default = "default_card_width")]
  pub card_width: u16,
  #[serde(default = "default_card_height")]
  pub card_height: u16,
  pub gap_x: Option<u16>,
  pub gap_y: Option<u16>,
  pub card_style: Option<String>,
  pub show_filename: Option<bool>,
  pub filename_position: Option<String>,
  pub image_alignment: Option<String>,
  pub image_ratio: Option<f32>,
  pub label_lines: Option<u16>,
  pub show_border: Option<bool>,
  pub padding: Option<u16>,
}

impl LayoutPresetConfig {
  fn grid() -> Self {
    Self {
      strategy: "grid".to_string(),
      params: vec!["columns".to_string(), "rows".to_string()],
      columns: 3,
      rows: 2,
      card_width: 34,
      card_height: 16,
      label_lines: Some(1),
      ..Self::default()
    }
  }

  fn list() -> Self {
    Self {
      strategy: "list".to_string(),
      params: vec!["items".to_string()],
      columns: 1,
      items: 12,
      card_height: 5,
      gap_y: Some(0),
      filename_position: Some("right".to_string()),
      image_alignment: Some("left".to_string()),
      image_ratio: Some(0.35),
      show_border: Some(false),
      ..Self::default()
    }
  }

  fn masonry() -> Self {
    Self {
      strategy: "masonry".to_string(),
      params: vec!["columns".to_string(), "card_width".to_string()],
      columns: 0,
      card_width: 34,
      card_height: 16,
      label_lines: Some(1),
      ..Self::default()
    }
  }
}

impl Default for LayoutPresetConfig {
  fn default() -> Self {
    Self {
      strategy: default_layout_strategy(),
      params: Vec::new(),
      columns: 0,
      rows: 0,
      items: 0,
      card_width: default_card_width(),
      card_height: default_card_height(),
      gap_x: None,
      gap_y: None,
      card_style: None,
      show_filename: None,
      filename_position: None,
      image_alignment: None,
      image_ratio: None,
      label_lines: None,
      show_border: None,
      padding: None,
    }
  }
}

fn default_layout_active() -> String {
  "grid".to_string()
}

fn default_layout_active_args() -> Vec<String> {
  vec!["3".to_string(), "2".to_string()]
}

fn default_gap_x() -> u16 {
  2
}

fn default_gap_y() -> u16 {
  1
}

fn default_card_style() -> String {
  "image_with_name".to_string()
}

fn default_show_filename() -> bool {
  true
}

fn default_filename_position() -> String {
  "bottom".to_string()
}

fn default_image_alignment() -> String {
  "center".to_string()
}

fn default_image_ratio() -> f32 {
  0.75
}

fn default_label_lines() -> u16 {
  0
}

fn default_show_border() -> bool {
  true
}

fn default_padding() -> u16 {
  1
}

fn default_layout_strategy() -> String {
  "grid".to_string()
}

fn default_card_width() -> u16 {
  34
}

fn default_card_height() -> u16 {
  16
}

fn default_layout_presets() -> BTreeMap<String, LayoutPresetConfig> {
  let mut presets = BTreeMap::new();
  presets.insert("grid".to_string(), LayoutPresetConfig::grid());
  presets.insert("list".to_string(), LayoutPresetConfig::list());
  presets.insert("masonry".to_string(), LayoutPresetConfig::masonry());
  presets
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveLayoutConfig {
  pub name: String,
  pub strategy: String,
  pub columns: u16,
  pub rows: u16,
  pub items: u16,
  pub card_width: u16,
  pub card_height: u16,
  pub gap_x: u16,
  pub gap_y: u16,
  pub card_style: String,
  pub show_filename: bool,
  pub filename_position: String,
  pub image_alignment: String,
  pub image_ratio: f32,
  pub label_lines: u16,
  pub show_border: bool,
  pub padding: u16,
}

impl EffectiveLayoutConfig {
  pub fn label(&self) -> String {
    match self.strategy.as_str() {
      "grid" | "fixed_grid" => {
        format!("{} {}x{}", self.name, self.columns.max(1), self.rows.max(1))
      }
      "list" => format!("{} {}", self.name, self.items.max(1)),
      "masonry" => {
        if self.columns == 0 {
          format!("{} auto {}", self.name, self.card_width.max(1))
        } else {
          format!("{} {} {}", self.name, self.columns, self.card_width.max(1))
        }
      }
      _ => self.name.clone(),
    }
  }
}

impl LayoutConfig {
  fn normalize_defaults(&mut self) {
    for (name, default_preset) in default_layout_presets() {
      match self.presets.get_mut(&name) {
        Some(preset) => preset.fill_missing_from(&default_preset),
        None => {
          self.presets.insert(name, default_preset);
        }
      }
    }
  }

  pub fn effective(&self) -> EffectiveLayoutConfig {
    self
      .effective_for(&self.active, &self.active_args)
      .unwrap_or_else(|_| default_effective_layout(self))
  }

  pub fn set_active_from_args(
    &mut self,
    name: &str,
    raw_args: &[&str],
  ) -> Result<EffectiveLayoutConfig, String> {
    let preset = self
      .presets
      .get(name)
      .ok_or_else(|| format!("unknown layout: {name}"))?;
    let args = normalize_layout_args(preset, raw_args)?;
    let effective = self.effective_for(name, &args)?;
    self.active = name.to_string();
    self.active_args = args;
    Ok(effective)
  }

  fn effective_for(
    &self,
    name: &str,
    raw_args: &[String],
  ) -> Result<EffectiveLayoutConfig, String> {
    let preset = self
      .presets
      .get(name)
      .ok_or_else(|| format!("unknown layout: {name}"))?;
    if raw_args.len() > preset.params.len() {
      return Err(layout_usage(name, preset));
    }

    let mut effective = EffectiveLayoutConfig {
      name: name.to_string(),
      strategy: normalize_layout_strategy(&preset.strategy),
      columns: preset.columns,
      rows: preset.rows,
      items: preset.items,
      card_width: preset.card_width,
      card_height: preset.card_height,
      gap_x: preset.gap_x.unwrap_or(self.gap_x),
      gap_y: preset.gap_y.unwrap_or(self.gap_y),
      card_style: preset
        .card_style
        .clone()
        .unwrap_or_else(|| self.card_style.clone()),
      show_filename: preset.show_filename.unwrap_or(self.show_filename),
      filename_position: preset
        .filename_position
        .clone()
        .unwrap_or_else(|| self.filename_position.clone()),
      image_alignment: preset
        .image_alignment
        .clone()
        .unwrap_or_else(|| self.image_alignment.clone()),
      image_ratio: preset.image_ratio.unwrap_or(self.image_ratio),
      label_lines: preset.label_lines.unwrap_or(self.label_lines),
      show_border: preset.show_border.unwrap_or(self.show_border),
      padding: preset.padding.unwrap_or(self.padding),
    };

    for (param, value) in preset.params.iter().zip(raw_args) {
      apply_layout_param(&mut effective, param, value)
        .map_err(|err| format!("{err}; {}", layout_usage(name, preset)))?;
    }
    normalize_effective_layout(&mut effective);
    Ok(effective)
  }
}

impl LayoutPresetConfig {
  fn fill_missing_from(&mut self, default: &LayoutPresetConfig) {
    if self.gap_x.is_none() {
      self.gap_x = default.gap_x;
    }
    if self.gap_y.is_none() {
      self.gap_y = default.gap_y;
    }
    if self.card_style.is_none() {
      self.card_style = default.card_style.clone();
    }
    if self.show_filename.is_none() {
      self.show_filename = default.show_filename;
    }
    if self.filename_position.is_none() {
      self.filename_position = default.filename_position.clone();
    }
    if self.image_alignment.is_none() {
      self.image_alignment = default.image_alignment.clone();
    }
    if self.image_ratio.is_none() {
      self.image_ratio = default.image_ratio;
    }
    if self.label_lines.is_none() {
      self.label_lines = default.label_lines;
    }
    if self.show_border.is_none() {
      self.show_border = default.show_border;
    }
    if self.padding.is_none() {
      self.padding = default.padding;
    }
  }
}

fn default_effective_layout(config: &LayoutConfig) -> EffectiveLayoutConfig {
  let fallback = LayoutPresetConfig::grid();
  let mut effective = EffectiveLayoutConfig {
    name: "grid".to_string(),
    strategy: fallback.strategy,
    columns: fallback.columns,
    rows: fallback.rows,
    items: fallback.items,
    card_width: fallback.card_width,
    card_height: fallback.card_height,
    gap_x: config.gap_x,
    gap_y: config.gap_y,
    card_style: config.card_style.clone(),
    show_filename: config.show_filename,
    filename_position: config.filename_position.clone(),
    image_alignment: config.image_alignment.clone(),
    image_ratio: config.image_ratio,
    label_lines: config.label_lines,
    show_border: config.show_border,
    padding: config.padding,
  };
  normalize_effective_layout(&mut effective);
  effective
}

fn normalize_layout_args(
  preset: &LayoutPresetConfig,
  raw_args: &[&str],
) -> Result<Vec<String>, String> {
  let mut args = raw_args
    .iter()
    .map(|arg| arg.trim().to_string())
    .filter(|arg| !arg.is_empty())
    .collect::<Vec<_>>();

  if args.len() == 1
    && preset.params.len() >= 2
    && is_param(&preset.params[0], &["columns", "column", "cols"])
    && is_param(&preset.params[1], &["rows", "row"])
    && let Some((columns, rows)) = split_grid_shape(&args[0])
  {
    args = vec![columns, rows];
  }

  if args.len() > preset.params.len() {
    return Err("too many layout arguments".to_string());
  }
  Ok(args)
}

fn split_grid_shape(value: &str) -> Option<StringPair> {
  let (columns, rows) = value.split_once('x').or_else(|| value.split_once('X'))?;
  let columns = columns.trim();
  let rows = rows.trim();
  if columns.is_empty() || rows.is_empty() {
    return None;
  }
  Some((columns.to_string(), rows.to_string()))
}

type StringPair = (String, String);

fn normalize_layout_strategy(strategy: &str) -> String {
  match strategy.trim().to_ascii_lowercase().as_str() {
    "fixed_grid" | "fixed-grid" | "grid" => "grid".to_string(),
    "list" => "list".to_string(),
    "masonry" | "dense" => "masonry".to_string(),
    other => other.to_string(),
  }
}

fn normalize_effective_layout(layout: &mut EffectiveLayoutConfig) {
  layout.strategy = normalize_layout_strategy(&layout.strategy);
  if layout.strategy == "grid" {
    layout.columns = layout.columns.max(1);
    layout.rows = layout.rows.max(1);
  } else if layout.strategy == "list" {
    layout.columns = 1;
    layout.items = layout.items.max(1);
  }
  layout.card_width = layout.card_width.max(1);
  layout.card_height = layout.card_height.max(1);
  layout.filename_position = match layout.filename_position.to_ascii_lowercase().as_str() {
    "top" | "bottom" | "left" | "right" => layout.filename_position.to_ascii_lowercase(),
    _ => "bottom".to_string(),
  };
  layout.image_alignment = match layout.image_alignment.to_ascii_lowercase().as_str() {
    "left" | "start" => "left".to_string(),
    "center" | "middle" => "center".to_string(),
    _ => "center".to_string(),
  };
  layout.image_ratio = layout.image_ratio.clamp(0.1, 0.95);
}

fn apply_layout_param(
  layout: &mut EffectiveLayoutConfig,
  param: &str,
  value: &str,
) -> Result<(), String> {
  match param.trim().to_ascii_lowercase().as_str() {
    "columns" | "column" | "cols" => layout.columns = parse_layout_u16(param, value)?,
    "rows" | "row" => layout.rows = parse_layout_u16(param, value)?,
    "items" | "item" | "page_size" | "page-size" | "per_page" | "per-page" => {
      layout.items = parse_layout_u16(param, value)?
    }
    "card_width" | "card-width" | "width" | "w" => {
      layout.card_width = parse_layout_u16(param, value)?
    }
    "card_height" | "card-height" | "height" | "h" => {
      layout.card_height = parse_layout_u16(param, value)?
    }
    "gap_x" | "gap-x" => layout.gap_x = parse_layout_u16(param, value)?,
    "gap_y" | "gap-y" => layout.gap_y = parse_layout_u16(param, value)?,
    "card_style" | "card-style" | "style" => layout.card_style = value.to_string(),
    "filename_position" | "filename-position" | "name_position" | "name-position" => {
      layout.filename_position = value.to_string()
    }
    "image_alignment" | "image-alignment" | "image_align" | "image-align" | "align" => {
      layout.image_alignment = value.to_string()
    }
    "image_ratio" | "image-ratio" | "image_size" | "image-size" | "ratio" => {
      layout.image_ratio = parse_layout_f32(param, value)?
    }
    "label_lines" | "label-lines" | "name_lines" | "name-lines" => {
      layout.label_lines = parse_layout_u16(param, value)?
    }
    "show_border" | "show-border" | "border" | "borders" => {
      layout.show_border = parse_layout_bool(param, value)?
    }
    "padding" | "pad" => layout.padding = parse_layout_u16(param, value)?,
    "show_filename" | "show-filename" | "name" => {
      layout.show_filename = parse_layout_bool(param, value)?
    }
    _ => return Err(format!("unknown layout parameter: {param}")),
  }
  Ok(())
}

fn parse_layout_u16(param: &str, value: &str) -> Result<u16, String> {
  value
    .parse::<u16>()
    .map_err(|_| format!("{param} must be a non-negative integer"))
}

fn parse_layout_bool(param: &str, value: &str) -> Result<bool, String> {
  match value.trim().to_ascii_lowercase().as_str() {
    "true" | "yes" | "on" | "1" => Ok(true),
    "false" | "no" | "off" | "0" => Ok(false),
    _ => Err(format!("{param} must be true or false")),
  }
}

fn parse_layout_f32(param: &str, value: &str) -> Result<f32, String> {
  value
    .parse::<f32>()
    .map_err(|_| format!("{param} must be a number"))
}

fn is_param(value: &str, aliases: &[&str]) -> bool {
  let value = value.trim().to_ascii_lowercase();
  aliases.iter().any(|alias| value == *alias)
}

fn layout_usage(name: &str, preset: &LayoutPresetConfig) -> String {
  if preset.params.is_empty() {
    format!("usage: :layout {name}")
  } else {
    let params = preset
      .params
      .iter()
      .map(|param| format!("<{param}>"))
      .collect::<Vec<_>>()
      .join(" ");
    format!("usage: :layout {name} {params}")
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
  pub chafa_bin: String,
  pub auto_detect: bool,
  pub chafa_args: Vec<String>,
  pub cache_max_bytes: u64,
  pub cache_compression_level: i32,
  pub cache_compression_threads: u32,
  pub max_concurrent: usize,
  pub chafa_threads: usize,
  pub preload_ahead: usize,
  pub preload_behind: usize,
  pub passthrough: Option<String>,
  pub zellij_sixel: String,
}

impl Default for RenderConfig {
  fn default() -> Self {
    Self {
      chafa_bin: "chafa".to_string(),
      auto_detect: true,
      chafa_args: vec![
        "--format=symbols".to_string(),
        "--colors=full".to_string(),
        "--symbols=block".to_string(),
        "--animate=off".to_string(),
        "--polite=on".to_string(),
      ],
      cache_max_bytes: 512 * 1024 * 1024,
      cache_compression_level: 3,
      cache_compression_threads: 2,
      max_concurrent: 4,
      chafa_threads: 1,
      preload_ahead: 6,
      preload_behind: 2,
      passthrough: None,
      zellij_sixel: "off".to_string(),
    }
  }
}

impl RenderConfig {
  pub fn apply_terminal_capability(&mut self, capability: &crate::capability::TerminalCapability) {
    self.chafa_args.retain(|arg| {
      !arg.starts_with("--format=")
        && !arg.starts_with("--colors=")
        && !arg.starts_with("--symbols=")
        && !arg.starts_with("--passthrough=")
    });
    self
      .chafa_args
      .insert(0, capability.symbols_arg().to_string());
    self
      .chafa_args
      .insert(0, capability.colors_arg().to_string());
    self.passthrough = capability.passthrough().map(str::to_string);
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
  pub scroll_lines: u16,
  pub select_moves_focus: bool,
}

impl Default for BehaviorConfig {
  fn default() -> Self {
    Self {
      scroll_lines: 4,
      select_moves_focus: true,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapConfig {
  pub browser: KeymapSection,
  pub detail: KeymapSection,
  #[serde(default = "default_input_keymap_section")]
  pub input: KeymapSection,
  pub global: KeymapSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KeymapSection {
  pub keymap: Vec<KeymapEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeymapEntry {
  pub on: KeymapOn,
  pub run: String,
  pub desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeymapOn {
  One(String),
  Many(Vec<String>),
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
          key("end", "end", "Go to last image"),
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
          key("e", "edit_metadata", "Edit metadata in $EDITOR"),
        ],
      },
      input: default_input_keymap_section(),
      global: KeymapSection {
        keymap: vec![
          key("r", "rename", "Rename image"),
          key(":", "command", "Enter command"),
        ],
      },
    }
  }
}

impl KeymapConfig {
  fn normalize_defaults(&mut self) {
    let default = KeymapConfig::default();
    append_missing_actions(&mut self.browser.keymap, &default.browser.keymap);
    append_missing_actions(&mut self.detail.keymap, &default.detail.keymap);
    append_missing_actions(&mut self.input.keymap, &default.input.keymap);
    append_missing_actions(&mut self.global.keymap, &default.global.keymap);
  }
}

fn append_missing_actions(entries: &mut Vec<KeymapEntry>, defaults: &[KeymapEntry]) {
  for default in defaults {
    if entries.iter().any(|entry| entry.run == default.run) {
      continue;
    }
    entries.push(default.clone());
  }
}

fn default_input_keymap_section() -> KeymapSection {
  KeymapSection {
    keymap: vec![
      key("esc", "cancel", "Cancel input"),
      key("enter", "submit", "Submit input"),
      key("backspace", "backspace", "Delete before cursor"),
      key("delete", "delete", "Delete under cursor"),
      key("left", "move_left", "Move cursor left"),
      key("right", "move_right", "Move cursor right"),
      key("home", "move_start", "Move cursor to start"),
      key("ctrl-a", "move_start", "Move cursor to start"),
      key("end", "move_end", "Move cursor to end"),
      key("ctrl-e", "move_end", "Move cursor to end"),
      key("ctrl-u", "kill_before_cursor", "Delete before cursor"),
      key("ctrl-k", "kill_after_cursor", "Delete after cursor"),
      key("tab", "completion_next", "Select next completion"),
      key(
        "backtab",
        "completion_previous",
        "Select previous completion",
      ),
      key("up", "history_previous", "Previous command history"),
      key("down", "history_next", "Next command history"),
      key("ctrl-g", "edit_in_editor", "Edit input in $EDITOR"),
    ],
  }
}

fn key(on: impl Into<KeymapOn>, run: &str, desc: &str) -> KeymapEntry {
  KeymapEntry {
    on: on.into(),
    run: run.to_string(),
    desc: desc.to_string(),
  }
}

impl From<&str> for KeymapOn {
  fn from(value: &str) -> Self {
    Self::One(value.to_string())
  }
}

impl<const N: usize> From<[&str; N]> for KeymapOn {
  fn from(value: [&str; N]) -> Self {
    Self::Many(value.into_iter().map(str::to_string).collect())
  }
}

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
      which_key_background: "black".to_string(),
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

impl ThemeConfig {
  fn normalize_defaults(&mut self) {}
}

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
  "green".to_string()
}

pub async fn load_or_create() -> Result<Settings> {
  let config_dir = dirs::config_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("gallery-tui");
  let cache_dir = dirs::cache_dir()
    .unwrap_or_else(|| PathBuf::from(".cache"))
    .join("gallery-tui");

  fs::create_dir_all(&config_dir)
    .await
    .with_context(|| format!("failed to create {}", config_dir.display()))?;
  fs::create_dir_all(&cache_dir)
    .await
    .with_context(|| format!("failed to create {}", cache_dir.display()))?;

  let config_path = config_dir.join("config.toml");
  let config = read_or_write_default(&config_path, AppConfig::default()).await?;
  let keymap =
    read_or_write_keymap_default(&config_dir.join("keymap.toml"), KeymapConfig::default()).await?;
  let theme = read_or_write_default(&config_dir.join("theme.toml"), ThemeConfig::default()).await?;

  Ok(Settings {
    config,
    keymap,
    theme,
    config_path,
    cache_dir,
  })
}

pub async fn write_app_config(path: &Path, config: &AppConfig) -> Result<()> {
  let body = app_config_toml(config)?;
  fs::write(path, body)
    .await
    .with_context(|| format!("failed to write {}", path.display()))
}

pub fn write_app_config_sync(path: &Path, config: &AppConfig) -> Result<()> {
  let body = app_config_toml(config)?;
  std::fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))
}

fn app_config_toml(config: &AppConfig) -> Result<String> {
  toml::to_string_pretty(config).map_err(Into::into)
}

async fn read_or_write_keymap_default(path: &Path, default: KeymapConfig) -> Result<KeymapConfig> {
  if !path.exists() {
    fs::write(path, format_keymap_toml(&default))
      .await
      .with_context(|| format!("failed to write {}", path.display()))?;
    return Ok(default);
  }
  let body = fs::read_to_string(path)
    .await
    .with_context(|| format!("failed to read {}", path.display()))?;
  let mut parsed: KeymapConfig =
    toml::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))?;
  parsed.normalize_defaults();
  let normalized = format_keymap_toml(&parsed);
  write_back_if_toml_changed(path, &body, &normalized).await?;
  Ok(parsed)
}

async fn read_or_write_default<T>(path: &Path, default: T) -> Result<T>
where
  T: Serialize + for<'de> Deserialize<'de> + Clone,
  T: NormalizeConfigDefaults,
{
  if !path.exists() {
    let body = toml::to_string_pretty(&default)?;
    fs::write(path, body)
      .await
      .with_context(|| format!("failed to write {}", path.display()))?;
    let mut default = default;
    default.normalize_defaults();
    return Ok(default);
  }
  let body = fs::read_to_string(path)
    .await
    .with_context(|| format!("failed to read {}", path.display()))?;
  let mut parsed: T =
    toml::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))?;
  parsed.normalize_defaults();
  let normalized = toml::to_string_pretty(&parsed)?;
  write_back_if_toml_changed(path, &body, &normalized).await?;
  Ok(parsed)
}

trait NormalizeConfigDefaults {
  fn normalize_defaults(&mut self);
}

impl NormalizeConfigDefaults for AppConfig {
  fn normalize_defaults(&mut self) {
    AppConfig::normalize_defaults(self);
  }
}

impl NormalizeConfigDefaults for ThemeConfig {
  fn normalize_defaults(&mut self) {
    ThemeConfig::normalize_defaults(self);
  }
}

async fn write_back_if_toml_changed(path: &Path, original: &str, normalized: &str) -> Result<()> {
  if toml_semantic_value(original) != toml_semantic_value(normalized) {
    fs::write(path, normalized)
      .await
      .with_context(|| format!("failed to update {}", path.display()))?;
  }
  Ok(())
}

fn toml_semantic_value(body: &str) -> Option<toml::Value> {
  toml::from_str(body).ok()
}

fn format_keymap_toml(config: &KeymapConfig) -> String {
  let mut out = String::new();
  push_keymap_section(&mut out, "browser", &config.browser);
  push_keymap_section(&mut out, "detail", &config.detail);
  push_keymap_section(&mut out, "input", &config.input);
  push_keymap_section(&mut out, "global", &config.global);
  out
}

fn push_keymap_section(out: &mut String, name: &str, section: &KeymapSection) {
  let _ = writeln!(out, "[{name}]");
  out.push_str("keymap = [\n");
  for entry in &section.keymap {
    let _ = writeln!(
      out,
      "  {{ on = {}, run = {}, desc = {} }},",
      format_keymap_on(&entry.on),
      toml_basic_string(&entry.run),
      toml_basic_string(&entry.desc)
    );
  }
  out.push_str("]\n\n");
}

fn format_keymap_on(on: &KeymapOn) -> String {
  match on {
    KeymapOn::One(value) => toml_basic_string(value),
    KeymapOn::Many(values) => {
      let keys = values
        .iter()
        .map(|value| toml_basic_string(value))
        .collect::<Vec<_>>()
        .join(", ");
      format!("[{keys}]")
    }
  }
}

fn toml_basic_string(value: &str) -> String {
  let mut out = String::with_capacity(value.len() + 2);
  out.push('"');
  for ch in value.chars() {
    match ch {
      '\\' => out.push_str("\\\\"),
      '"' => out.push_str("\\\""),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      '\u{08}' => out.push_str("\\b"),
      '\u{0c}' => out.push_str("\\f"),
      ch if ch.is_control() => {
        let _ = write!(out, "\\u{:04X}", ch as u32);
      }
      ch => out.push(ch),
    }
  }
  out.push('"');
  out
}

pub fn sort_for_command(field: &str, direction: &str) -> Option<SortSpec> {
  let field = field.trim();
  if field.is_empty() {
    return None;
  }
  let field = match field.to_ascii_lowercase().as_str() {
    "name" | "filename" | "file" => SortField::Name,
    "path" => SortField::Path,
    "modified" | "mtime" => SortField::Modified,
    "created" | "ctime" => SortField::Created,
    "size" => SortField::Size,
    "format" | "extension" | "ext" => SortField::Format,
    "dimensions" | "dimension" | "resolution" => SortField::Dimensions,
    "metadata" | "exif" => SortField::MetadataCount,
    _ => SortField::Metadata(field.to_string()),
  };
  let direction = match direction.trim().to_ascii_lowercase().as_str() {
    "asc" | "ascending" => SortDirection::Asc,
    "desc" | "descending" => SortDirection::Desc,
    _ => return None,
  };
  Some(SortSpec { field, direction })
}

fn parse_color(value: &str) -> Color {
  let lower = value.trim().to_ascii_lowercase();
  match lower.as_str() {
    "reset" => Color::Reset,
    "black" => Color::Black,
    "red" => Color::Red,
    "green" => Color::Green,
    "yellow" => Color::Yellow,
    "blue" => Color::Blue,
    "magenta" => Color::Magenta,
    "cyan" => Color::Cyan,
    "gray" | "grey" => Color::Gray,
    "dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => Color::DarkGray,
    "light_red" | "lightred" => Color::LightRed,
    "light_green" | "lightgreen" => Color::LightGreen,
    "light_yellow" | "lightyellow" => Color::LightYellow,
    "light_blue" | "lightblue" => Color::LightBlue,
    "light_magenta" | "lightmagenta" => Color::LightMagenta,
    "light_cyan" | "lightcyan" => Color::LightCyan,
    "white" => Color::White,
    _ => {
      if let Some(raw) = lower.strip_prefix("ansi:") {
        return raw
          .parse::<u8>()
          .map(Color::Indexed)
          .unwrap_or(Color::Reset);
      }
      if lower.len() == 7 && lower.starts_with('#') {
        let r = u8::from_str_radix(&lower[1..3], 16);
        let g = u8::from_str_radix(&lower[3..5], 16);
        let b = u8::from_str_radix(&lower[5..7], 16);
        if let (Ok(r), Ok(g), Ok(b)) = (r, g, b) {
          return Color::Rgb(r, g, b);
        }
      }
      Color::Reset
    }
  }
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

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
