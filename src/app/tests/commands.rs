use super::*;

#[test]
fn sort_command_updates_sort_spec() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();

  app.execute_command("sort created desc".to_string(), &tx);

  assert_eq!(app.sort_spec.field, SortField::Created);
  assert_eq!(app.sort_spec.direction, SortDirection::Desc);
}

#[test]
fn sort_command_accepts_metadata_fields() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();

  app.execute_command("sort Exif.ExposureTime desc".to_string(), &tx);

  assert_eq!(
    app.sort_spec.field,
    SortField::Metadata("Exif.ExposureTime".to_string())
  );
  assert_eq!(app.sort_spec.direction, SortDirection::Desc);
}

#[test]
fn keymap_sort_action_uses_sort_command_syntax() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();

  app.handle_action("sort name desc", &tx);

  assert_eq!(app.sort_spec.field, SortField::Name);
  assert_eq!(app.sort_spec.direction, SortDirection::Desc);
}

#[test]
fn layout_command_updates_active_layout() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.settings.config_path = unique_config_path("layout-save");

  app.execute_command("layout grid 3 3".to_string(), &tx);

  let layout = app.settings.config.layout.effective();
  assert_eq!(layout.strategy, "grid");
  assert_eq!(layout.columns, 3);
  assert_eq!(layout.rows, 3);
  assert_eq!(app.message, "layout: grid 3x3 (saving)");

  let AsyncEvent::ConfigSave(outcome) = rx.try_recv().unwrap() else {
    panic!("expected config save outcome");
  };
  app.finish_config_save(outcome);
  assert_eq!(app.message, "layout saved: grid 3x3");
  let body = std::fs::read_to_string(&app.settings.config_path).unwrap();
  assert!(body.contains(r#"active = "grid""#));
  assert!(body.contains(r#""3""#));
}

#[test]
fn keymap_layout_action_uses_layout_command_syntax() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();

  app.handle_action("layout-use list 8", &tx);

  let layout = app.settings.config.layout.effective();
  assert_eq!(layout.strategy, "list");
  assert_eq!(layout.items, 8);
  assert_eq!(app.message, "layout: list 8 (temporary)");
}
