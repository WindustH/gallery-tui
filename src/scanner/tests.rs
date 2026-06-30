use super::*;
use std::time::SystemTime;

fn temp_path(name: &str) -> PathBuf {
  let unique = format!(
    "gallery-tui-scanner-test-{name}-{}-{}",
    std::process::id(),
    SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  );
  std::env::temp_dir().join(unique)
}

#[test]
fn missing_path_is_skipped() {
  let item = image_item_from_path(temp_path("missing").join("gone.png"), "png".to_string());

  assert!(item.is_none());
}

#[test]
fn rotated_orientation_flips_layout_dimensions() {
  assert_eq!(
    apply_orientation_to_dimensions((4000, 3000), image::metadata::Orientation::Rotate90),
    (3000, 4000)
  );
}

#[test]
fn unreadable_image_data_still_keeps_file_metadata() {
  let dir = temp_path("invalid-image");
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("not-really.png");
  std::fs::write(&path, b"not image data").unwrap();

  let item = image_item_from_path(path, "png".to_string()).unwrap();

  assert_eq!(item.file_name, "not-really.png");
  assert_eq!(item.dimensions, None);

  std::fs::remove_dir_all(&dir).unwrap();
}
