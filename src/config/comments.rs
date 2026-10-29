//! `config.toml` serialization with an explanatory comment above each field.

use anyhow::Result;

use super::AppConfig;

pub(super) fn app_config_toml(config: &AppConfig) -> Result<String> {
  let body = toml::to_string_pretty(config)?;
  Ok(framework_tui::commented_toml::add_comments(
    &body,
    &[
      "gallery-tui main configuration.",
      "Missing fields are rewritten with defaults when the app loads this file.",
    ],
    gallery_config_comment,
    comment_table_key,
  ))
}

fn comment_table_key(table: &str) -> String {
  if table.starts_with("layout.presets.") {
    let suffix = table.strip_prefix("layout.presets.").unwrap_or("");
    suffix.split_once('.').map_or_else(
      || "layout.presets.*".to_string(),
      |(_, nested)| format!("layout.presets.*.{nested}"),
    )
  } else {
    table.to_string()
  }
}

fn gallery_config_comment(key: &str) -> Option<&'static str> {
  match key {
    "layout.highlight" => Some("Show focus and selection styling on image cards by default."),
    "layout.presets.*.highlight" => Some(
      "Override card highlighting for this layout/panel; false keeps neutral colors without changing selection.",
    ),
    "layout.show_page_numbers" | "layout.presets.*.show_page_numbers" => {
      Some("Show a one-based item number above each page/image; independent of borders.")
    }
    "layout.presets.*.composition" => {
      Some("Combine two ordinary layout presets; each keeps its own geometry and cache sizes.")
    }
    "layout.presets.*.composition.direction" => {
      Some("horizontal: left/right; vertical: top/bottom.")
    }
    "layout.presets.*.composition.ratio" => {
      Some("First and second panel size weights, both positive. Gap is excluded before splitting.")
    }
    "layout.presets.*.composition.gap" => {
      Some("Blank columns (horizontal) or rows (vertical) between panels. Zero is allowed.")
    }
    "layout.presets.*.composition.primary" => {
      Some("first or second: keyboard/wheel control this panel; the other follows its progress.")
    }
    "layout.presets.*.composition.first.layout" | "layout.presets.*.composition.second.layout" => {
      Some("Name of an ordinary layout preset; nested compositions are not supported.")
    }
    "layout.presets.*.composition.first.args" | "layout.presets.*.composition.second.args" => {
      Some("Arguments for the referenced preset, in that preset's params order.")
    }

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
    "layout.scroll_divisor" | "layout.presets.*.scroll_divisor" => {
      Some("Slice height limit = viewport rows / this value (minimum one row); default 8.")
    }
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
    "layout.presets.*.strategy" => Some(
      "Layout algorithm used by this preset: grid, list, masonry, or composite (slides is a composite preset).",
    ),
    "layout.presets.*.params" => Some("Runtime arguments accepted by :layout for this preset."),
    "layout.presets.*.columns" => Some("Default column count for grid and masonry layouts."),
    "layout.presets.*.left_ratio" => {
      Some("Left thumbnail panel width weight for slides (positive integer).")
    }
    "layout.presets.*.right_ratio" => {
      Some("Right preview panel width weight for slides (positive integer).")
    }
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
    "render.raw_memory_cache_max_bytes" => Some(
      "Maximum RAM shared by decoded source images and uncompressed renders. 0 means unlimited.",
    ),
    "render.compressed_memory_cache_max_bytes" => {
      Some("Maximum RAM used for compressed rendered image data. 0 means unlimited.")
    }
    "render.disk_cache_max_bytes" => {
      Some("Maximum live SQLite cache payload bytes, enforced on writes. 0 means unlimited.")
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
