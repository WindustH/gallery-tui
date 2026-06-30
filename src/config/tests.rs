use super::*;

#[test]
fn compact_keymap_toml_round_trips() {
  let config = KeymapConfig::default();
  let body = format_keymap_toml(&config);

  assert!(!body.contains("[[browser.keymap]]"));
  assert!(body.contains(r#"{ on = ["c", "p"], run = "copy_paths""#));

  let parsed: KeymapConfig = toml::from_str(&body).unwrap();

  assert_eq!(parsed.browser.keymap.len(), config.browser.keymap.len());
  assert_eq!(parsed.detail.keymap.len(), config.detail.keymap.len());
  assert_eq!(parsed.input.keymap.len(), config.input.keymap.len());
  assert_eq!(parsed.global.keymap.len(), config.global.keymap.len());
}

#[test]
fn default_app_config_toml_round_trips() {
  let body = toml::to_string_pretty(&AppConfig::default()).unwrap();

  assert!(body.contains("[layout.presets.grid]"));
  assert!(body.contains("active_args = ["));
  assert!(body.contains("params = ["));
  assert!(body.contains(r#""columns""#));
  assert!(body.contains(r#""rows""#));

  let parsed: AppConfig = toml::from_str(&body).unwrap();
  let layout = parsed.layout.effective();

  assert_eq!(layout.strategy, "grid");
  assert_eq!(layout.columns, 3);
  assert_eq!(layout.rows, 2);
  assert_eq!(layout.label_lines, 1);
  assert_eq!(layout.padding, 1);
}

#[cfg(unix)]
#[test]
fn unix_config_dir_prefers_xdg_then_home() {
  assert_eq!(
    unix_platform_dir(
      Some(PathBuf::from("/tmp/xdg-config")),
      Some(PathBuf::from("/tmp/home")),
      Some(PathBuf::from("/tmp/fallback")),
      ".config",
      ".",
    ),
    PathBuf::from("/tmp/xdg-config")
  );
  assert_eq!(
    unix_platform_dir(
      None,
      Some(PathBuf::from("/tmp/home")),
      Some(PathBuf::from("/tmp/fallback")),
      ".config",
      ".",
    ),
    PathBuf::from("/tmp/home/.config")
  );
}

#[cfg(unix)]
#[test]
fn unix_cache_dir_prefers_xdg_then_home() {
  assert_eq!(
    unix_platform_dir(
      Some(PathBuf::from("/tmp/xdg-cache")),
      Some(PathBuf::from("/tmp/home")),
      Some(PathBuf::from("/tmp/fallback")),
      ".cache",
      ".cache",
    ),
    PathBuf::from("/tmp/xdg-cache")
  );
  assert_eq!(
    unix_platform_dir(
      None,
      Some(PathBuf::from("/tmp/home")),
      Some(PathBuf::from("/tmp/fallback")),
      ".cache",
      ".cache",
    ),
    PathBuf::from("/tmp/home/.cache")
  );
}

#[test]
fn default_theme_uses_plain_selected_colors() {
  let theme = ThemeConfig::default();

  assert_eq!(theme.selected_foreground, "auto");
  assert_eq!(theme.selected_background, "white");
  assert_eq!(theme.hover_foreground, "black");
  assert_eq!(theme.hover_background, "cyan");
  assert_eq!(theme.hover_selected_foreground, "black");
  assert_eq!(theme.hover_selected_background, "green");
  assert_eq!(
    theme.foreground_color(&theme.selected_foreground, &theme.selected_background),
    Color::Black
  );
  assert_eq!(
    theme.foreground_color(&theme.hover_foreground, &theme.hover_background),
    Color::Black
  );
  assert_eq!(
    theme.foreground_color(&theme.selected_foreground, "ansi:236"),
    Color::White
  );
}

#[test]
fn default_sort_keymap_keeps_only_common_shortcuts() {
  let config = KeymapConfig::default();
  let body = format_keymap_toml(&config);

  assert!(body.contains(r#"{ on = ["s", "n"], run = "sort name asc""#));
  assert!(body.contains(r#"{ on = ["s", "m"], run = "sort modified asc""#));
  assert!(body.contains(r#"{ on = ["s", "S"], run = "sort size desc""#));
  assert!(body.contains(r#"{ on = "e", run = "edit_metadata""#));
  assert!(body.contains(r#"{ on = "ctrl-g", run = "edit_in_editor""#));
  assert!(!body.contains("sort_name_asc"));
  assert!(!body.contains("sort_modified_asc"));
  assert!(!body.contains("sort_size_desc"));
  assert!(!body.contains(r#"{ on = ["s", "c"]"#));
  assert!(!body.contains(r#"{ on = ["s", "f"]"#));
}

#[test]
fn sort_command_parses_extended_fields() {
  assert_eq!(
    sort_for_command("created", "desc"),
    Some(SortSpec {
      field: SortField::Created,
      direction: SortDirection::Desc,
    })
  );
  assert_eq!(
    sort_for_command("format", "asc"),
    Some(SortSpec {
      field: SortField::Format,
      direction: SortDirection::Asc,
    })
  );
  assert_eq!(
    sort_for_command("Exif.ExposureTime", "desc"),
    Some(SortSpec {
      field: SortField::Metadata("Exif.ExposureTime".to_string()),
      direction: SortDirection::Desc,
    })
  );
  assert_eq!(sort_for_command("format", "sideways"), None);
}

#[test]
fn layout_command_applies_positional_arguments() {
  let mut config = LayoutConfig::default();
  let layout = config
    .set_active_from_args("grid", &["3", "3"])
    .expect("grid layout args should parse");

  assert_eq!(config.active, "grid");
  assert_eq!(config.active_args, vec!["3", "3"]);
  assert_eq!(layout.strategy, "grid");
  assert_eq!(layout.columns, 3);
  assert_eq!(layout.rows, 3);
}

#[test]
fn layout_command_accepts_compact_grid_shape() {
  let mut config = LayoutConfig::default();
  let layout = config
    .set_active_from_args("grid", &["4x2"])
    .expect("compact grid shape should parse");

  assert_eq!(layout.columns, 4);
  assert_eq!(layout.rows, 2);
}

#[test]
fn layout_command_uses_preset_parameter_list() {
  let mut config = LayoutConfig::default();
  let layout = config
    .set_active_from_args("list", &["12"])
    .expect("list layout args should parse");

  assert_eq!(layout.strategy, "list");
  assert_eq!(layout.columns, 1);
  assert_eq!(layout.items, 12);
  assert_eq!(layout.gap_y, 0);
  assert_eq!(layout.filename_position, "right");
  assert_eq!(layout.image_alignment, "left");
  assert!((layout.image_ratio - 0.35).abs() < f32::EPSILON);
  assert_eq!(layout.label_lines, 0);
  assert!(!layout.show_border);
  assert_eq!(layout.padding, 1);
}

#[test]
fn layout_command_can_override_padding() {
  let mut config = LayoutConfig::default();
  config
    .presets
    .get_mut("list")
    .unwrap()
    .params
    .push("padding".to_string());

  let layout = config
    .set_active_from_args("list", &["12", "2"])
    .expect("list layout padding arg should parse");

  assert_eq!(layout.strategy, "list");
  assert!(!layout.show_border);
  assert_eq!(layout.padding, 2);
}
