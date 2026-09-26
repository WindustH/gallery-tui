//! `config.toml` serialization with an explanatory comment above each field.

use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::Result;

use super::AppConfig;

pub(super) fn app_config_toml(config: &AppConfig) -> Result<String> {
  let body = toml::to_string_pretty(config)?;
  Ok(add_app_config_comments(
    &body,
    &[
      "gallery-tui main configuration.",
      "Missing fields are rewritten with defaults when the app loads this file.",
    ],
    gallery_config_comment,
  ))
}

fn add_app_config_comments(
  body: &str,
  header: &[&str],
  comment_for: fn(&str) -> Option<&'static str>,
) -> String {
  let mut out = String::new();
  let mut seen_comments = BTreeSet::new();
  for line in header {
    push_toml_comment(&mut out, line);
  }
  out.push('\n');

  let mut table = String::new();
  for line in body.lines() {
    let trimmed = line.trim();
    if let Some(header) = toml_table_header(trimmed) {
      table = header.to_string();
      let comment_key = comment_table_key(&table);
      if seen_comments.insert(comment_key.clone())
        && let Some(comment) = comment_for(&comment_key)
      {
        push_toml_comment(&mut out, comment);
      }
    } else if let Some(key) = toml_field_key(trimmed) {
      let comment_key = comment_field_key(&table, key);
      if seen_comments.insert(comment_key.clone())
        && let Some(comment) = comment_for(&comment_key)
      {
        push_toml_comment(&mut out, comment);
      }
    }
    out.push_str(line);
    out.push('\n');
  }
  out
}

fn push_toml_comment(out: &mut String, comment: &str) {
  for line in comment.lines() {
    let _ = writeln!(out, "# {line}");
  }
}

fn toml_table_header(line: &str) -> Option<&str> {
  if line.starts_with("[[") {
    return None;
  }
  line.strip_prefix('[')?.strip_suffix(']')
}

fn toml_field_key(line: &str) -> Option<&str> {
  if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
    return None;
  }
  let (key, _) = line.split_once('=')?;
  let key = key.trim();
  (!key.is_empty()).then_some(key)
}

fn comment_table_key(table: &str) -> String {
  if table.starts_with("layout.presets.") {
    "layout.presets.*".to_string()
  } else {
    table.to_string()
  }
}

fn comment_field_key(table: &str, key: &str) -> String {
  if table.is_empty() {
    key.to_string()
  } else if table.starts_with("layout.presets.") {
    format!("layout.presets.*.{key}")
  } else {
    format!("{table}.{key}")
  }
}

fn gallery_config_comment(key: &str) -> Option<&'static str> {
  match key {
    "recursive" => Some("Scan image files in subdirectories as well as the current directory."),
    "initial_sort" => Some(
      "Initial browser sort order. Common values: name_asc, name_desc, modified_desc, size_asc.",
    ),
    "supported_extensions" => {
      Some("File extensions treated as images, matched case-insensitively without the dot.")
    }
    "layout" => Some("Layout defaults and the startup preset selection."),
    "layout.active" => Some("Layout preset selected on startup."),
    "layout.active_args" => Some(
      "Arguments passed to the active layout preset, in the order declared by that preset's params.",
    ),
    "layout.gap_x" => Some("Default horizontal gap between image cards."),
    "layout.gap_y" => Some("Default vertical gap between image cards."),
    "layout.card_style" => {
      Some("Default card style. image_with_name shows the image and filename together.")
    }
    "layout.show_filename" => Some("Show filenames in image cards."),
    "layout.filename_position" => Some("Default filename position: top, bottom, left, or right."),
    "layout.image_alignment" => Some("Default image alignment inside each card: left or center."),
    "layout.image_ratio" => {
      Some("Default fraction of a card reserved for the image when text is also shown.")
    }
    "layout.label_lines" => {
      Some("Default filename lines reserved for top/bottom labels. 0 sizes them from image_ratio.")
    }
    "layout.show_border" => Some("Draw borders around image cards by default."),
    "layout.padding" => Some("Default inner padding for image cards."),
    "layout.presets.*" => Some("Named layout preset used by the :layout command."),
    "layout.presets.*.strategy" => {
      Some("Layout algorithm used by this preset: grid, list, or masonry.")
    }
    "layout.presets.*.params" => Some("Runtime arguments accepted by :layout for this preset."),
    "layout.presets.*.columns" => Some("Default column count for grid and masonry layouts."),
    "layout.presets.*.rows" => Some("Default row count for fixed grid layouts."),
    "layout.presets.*.items" => Some("Default item count for list layouts."),
    "layout.presets.*.card_width" => Some("Card width in cells for masonry layouts."),
    "layout.presets.*.card_height" => {
      Some("Masonry card height in cells for images whose size is unknown.")
    }
    "layout.presets.*.gap_x" => Some("Override the global horizontal gap for this preset."),
    "layout.presets.*.gap_y" => Some("Override the global vertical gap for this preset."),
    "layout.presets.*.card_style" => Some("Override the global card style for this preset."),
    "layout.presets.*.show_filename" => {
      Some("Override whether filenames are shown for this preset.")
    }
    "layout.presets.*.filename_position" => Some("Override the filename position for this preset."),
    "layout.presets.*.image_alignment" => Some("Override the image alignment for this preset."),
    "layout.presets.*.image_ratio" => {
      Some("Override the image-to-text size ratio for this preset.")
    }
    "layout.presets.*.label_lines" => {
      Some("Override the number of filename lines reserved by this preset.")
    }
    "layout.presets.*.show_border" => Some("Override whether this preset draws card borders."),
    "layout.presets.*.padding" => Some("Override the card padding for this preset."),
    "render" => Some("Rendering, terminal graphics, preloading, and cache settings."),
    "render.chafa_bin" => Some("Chafa executable used for the text (symbols and ASCII) fallback."),
    "render.auto_detect" => Some(
      "Detect terminal graphics support to pick render modes and Chafa colors. When false, only Chafa text output is used.",
    ),
    "render.chafa_args" => Some(
      "Arguments passed to Chafa. With auto_detect, --colors and --symbols come from the terminal.",
    ),
    "render.raw_memory_cache_max_bytes" => {
      Some("Maximum RAM used for uncompressed rendered image data. 0 means unlimited.")
    }
    "render.compressed_memory_cache_max_bytes" => {
      Some("Maximum RAM used for compressed rendered image data. 0 means unlimited.")
    }
    "render.disk_cache_max_bytes" => {
      Some("Maximum disk space used for the render cache, enforced at startup. 0 means unlimited.")
    }
    "render.cache_compression_level" => Some("Compression level used for cached render data."),
    "render.cache_compression_threads" => {
      Some("Worker threads used when compressing cache entries.")
    }
    "render.max_concurrent" => Some("Maximum number of images rendered concurrently."),
    "render.chafa_threads" => Some("Threads requested per Chafa render job."),
    "render.preload_ahead" => Some("Number of images ahead of the current selection to preload."),
    "render.preload_behind" => {
      Some("Number of images behind the current selection to keep preloaded.")
    }
    "render.passthrough" => Some("Unused; tmux and screen passthrough is detected automatically."),
    "render.zellij_sixel" => Some("Zellij SIXEL handling mode."),
    "behavior" => Some("Interactive behavior settings."),
    "behavior.scroll_lines" => Some("Unused; kept so older config files stay valid."),
    "behavior.select_moves_focus" => {
      Some("Move keyboard focus with the selected image in browser views.")
    }
    "behavior.frame_sync_navigation" => {
      Some("Apply at most one navigation step per drawn frame, dropping extra queued key repeats.")
    }
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn app_config_toml_writes_parseable_commented_defaults() {
    let body = app_config_toml(&AppConfig::default()).expect("default config should serialize");
    toml::from_str::<AppConfig>(&body).expect("commented default config should parse");
    assert!(body.contains("# gallery-tui main configuration."));
    assert!(body.contains("# Layout preset selected on startup."));
  }

  #[test]
  fn app_config_toml_deduplicates_preset_field_comments() {
    let body = app_config_toml(&AppConfig::default()).expect("default config should serialize");
    assert_eq!(
      body
        .matches("# Layout algorithm used by this preset")
        .count(),
      1
    );
    assert_eq!(
      body
        .matches("# Card width in cells for masonry layouts.")
        .count(),
      1
    );
  }
}
