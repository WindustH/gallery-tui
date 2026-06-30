use super::*;

#[test]
fn missing_or_unsupported_metadata_returns_empty_list() {
  let entries = read_image_metadata(Path::new("/tmp/gallery-tui-missing-metadata.jpg"));

  assert!(entries.is_empty());
}

#[test]
fn metadata_edit_detects_changed_tags() {
  let original = vec![ImageMetadataEntry {
    group: "Exif".to_string(),
    name: "Artist".to_string(),
    value: "old".to_string(),
  }];
  let edit =
    metadata_changes_from_edit("image.jpg", &original, "[tags]\nArtist = \"new\"\n").unwrap();

  assert_eq!(edit.file_name, None);
  assert_eq!(
    edit.tags,
    vec![MetadataChange {
      tag: "Artist".to_string(),
      old_value: Some("old".to_string()),
      new_value: "new".to_string(),
    }]
  );
}

#[test]
fn metadata_edit_detects_changed_filename() {
  let edit =
    metadata_changes_from_edit("old.jpg", &[], "[file]\nname = \"new.jpg\"\n\n[tags]\n").unwrap();

  assert_eq!(
    edit.file_name,
    Some(FileNameChange {
      old_value: "old.jpg".to_string(),
      new_value: "new.jpg".to_string(),
    })
  );
  assert!(edit.tags.is_empty());
}
