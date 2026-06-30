use super::*;

#[test]
fn command_history_uses_up_and_down() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();

  app.prompt = Some(Prompt::command("sort name asc"));
  app.handle_prompt_key(prompt_key(KeyCode::Enter), &tx);
  app.prompt = Some(Prompt::command("draft"));

  app.handle_prompt_key(prompt_key(KeyCode::Up), &tx);
  assert_eq!(command_input(&app), "sort name asc");

  app.handle_prompt_key(prompt_key(KeyCode::Down), &tx);
  assert_eq!(command_input(&app), "draft");
}

#[test]
fn command_completion_completes_layout_name() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.prompt = Some(Prompt::command("layout gr"));

  app.handle_prompt_key(prompt_key(KeyCode::Tab), &tx);
  assert_eq!(command_input(&app), "layout gr");

  app.handle_prompt_key(prompt_key(KeyCode::Enter), &tx);
  assert_eq!(command_input(&app), "layout grid ");
}

#[test]
fn command_completion_uses_metadata_sort_fields() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  let mut item = image("a.png");
  item.metadata.push(ImageMetadataEntry {
    group: "Exif".to_string(),
    name: "ISO".to_string(),
    value: "100".to_string(),
  });
  app.images = vec![item];
  app.prompt = Some(Prompt::command("sort Exif.I"));

  app.handle_prompt_key(prompt_key(KeyCode::Tab), &tx);
  assert_eq!(command_input(&app), "sort Exif.I");

  app.handle_prompt_key(prompt_key(KeyCode::Enter), &tx);
  assert_eq!(command_input(&app), "sort Exif.ISO ");
}

#[test]
fn command_completion_cycles_candidates_with_tab_and_backtab() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.prompt = Some(Prompt::command(String::new()));
  app.refresh_command_completion();

  let initial = app
    .command_completion
    .as_ref()
    .and_then(CommandCompletion::selected_candidate)
    .cloned();
  app.handle_prompt_key(prompt_key(KeyCode::Tab), &tx);
  let next = app
    .command_completion
    .as_ref()
    .and_then(CommandCompletion::selected_candidate)
    .cloned();
  app.handle_prompt_key(prompt_key(KeyCode::BackTab), &tx);
  let previous = app
    .command_completion
    .as_ref()
    .and_then(CommandCompletion::selected_candidate)
    .cloned();

  assert_ne!(initial, next);
  assert_eq!(initial, previous);
}

#[test]
fn prompt_input_moves_cursor_and_edits_at_cursor() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.prompt = Some(Prompt::command("abc"));

  app.handle_prompt_key(prompt_key(KeyCode::Left), &tx);
  app.handle_prompt_key(prompt_key(KeyCode::Left), &tx);
  app.handle_prompt_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE), &tx);

  assert_eq!(command_input(&app), "aXbc");
  assert_eq!(app.command_buffer().unwrap().cursor, 2);

  app.handle_prompt_key(
    prompt_key_with(KeyCode::Char('a'), KeyModifiers::CONTROL),
    &tx,
  );
  app.handle_prompt_key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE), &tx);
  assert_eq!(command_input(&app), "YaXbc");

  app.handle_prompt_key(
    prompt_key_with(KeyCode::Char('e'), KeyModifiers::CONTROL),
    &tx,
  );
  app.handle_prompt_key(
    prompt_key_with(KeyCode::Char('u'), KeyModifiers::CONTROL),
    &tx,
  );
  assert_eq!(command_input(&app), "");
}

#[test]
fn prompt_input_requests_external_editor() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.prompt = Some(Prompt::command("draft"));

  app.handle_prompt_key(
    prompt_key_with(KeyCode::Char('g'), KeyModifiers::CONTROL),
    &tx,
  );

  let Some(EditorRequest::Prompt { input }) = app.take_editor_request() else {
    panic!("expected prompt editor request");
  };
  assert_eq!(input, "draft");
}

#[test]
fn rename_prompt_places_cursor_before_extension() {
  let mut app = test_app();
  app.images = vec![image("photo.final.jpg")];

  app.start_rename();

  let buffer = rename_buffer(&app);
  assert_eq!(buffer.input, "photo.final.jpg");
  assert_eq!(buffer.cursor, "photo.final".len());
}
