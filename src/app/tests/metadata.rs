use super::*;

#[test]
fn metadata_editor_can_request_filename_change() {
  let mut app = test_app();
  let path = PathBuf::from("/tmp/gallery-tui-test-root/old.jpg");

  app.finish_metadata_editor_input(
    path.clone(),
    Vec::new(),
    Ok("[file]\nname = \"new.jpg\"\n\n[tags]\n".to_string()),
  );

  let Some(ConfirmDialog::MetadataWrite {
    path: actual_path,
    edit,
  }) = app.confirm
  else {
    panic!("expected metadata confirmation");
  };
  assert_eq!(actual_path, path);
  let change = edit.file_name.unwrap();
  assert_eq!(change.old_value, "old.jpg");
  assert_eq!(change.new_value, "new.jpg");
  assert!(edit.tags.is_empty());
}

#[test]
fn confirm_enter_cancels_by_default() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.confirm = Some(ConfirmDialog::MetadataWrite {
    path: PathBuf::from("/tmp/gallery-tui-test-root/old.jpg"),
    edit: MetadataEdit::default(),
  });

  app.handle_input(
    Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    &tx,
  );

  assert!(app.confirm.is_none());
  assert_eq!(app.message, "cancelled");
}

#[test]
fn rename_rejects_existing_target() {
  let dir = unique_temp_dir("rename-existing-target");
  std::fs::create_dir_all(&dir).unwrap();
  let source = dir.join("old.jpg");
  let target = dir.join("new.jpg");
  std::fs::write(&source, b"old").unwrap();
  std::fs::write(&target, b"new").unwrap();

  let error = validate_new_file_name(&source, "new.jpg").unwrap_err();

  assert!(error.contains("target already exists"));
  let _ = std::fs::remove_file(source);
  let _ = std::fs::remove_file(target);
  let _ = std::fs::remove_dir(dir);
}
