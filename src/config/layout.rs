//! Browser layout presets and the `:layout` argument handling.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
      gap_x: Some(0),
      gap_y: Some(0),
      label_lines: Some(1),
      show_border: Some(false),
      padding: Some(1),
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
      padding: Some(0),
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
      show_border: Some(false),
      padding: Some(1),
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
  vec!["4".to_string(), "2".to_string()]
}

fn default_gap_x() -> u16 {
  0
}

fn default_gap_y() -> u16 {
  0
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
  /// Add built-in presets missing from the file and fill their unset fields.
  pub(super) fn normalize_defaults(&mut self) {
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

  /// Check that the startup layout and its arguments resolve.
  pub(super) fn validate_active(&self) -> Result<(), String> {
    self
      .effective_for(&self.active, &self.active_args)
      .map(|_| ())
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
