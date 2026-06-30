use super::*;

fn item(name: &str, metadata: Vec<(&str, &str, &str)>) -> ImageItem {
  ImageItem {
    path: PathBuf::from(format!("/tmp/{name}")),
    file_name: name.to_string(),
    extension: "jpg".to_string(),
    size_bytes: 1,
    modified: Some(UNIX_EPOCH),
    created: Some(UNIX_EPOCH),
    dimensions: Some((10, 10)),
    metadata: metadata
      .into_iter()
      .map(|(group, name, value)| ImageMetadataEntry {
        group: group.to_string(),
        name: name.to_string(),
        value: value.to_string(),
      })
      .collect(),
  }
}

#[test]
fn sorts_by_metadata_tag_name_as_number() {
  let mut images = vec![
    item("high.jpg", vec![("Exif", "ISO", "800")]),
    item("low.jpg", vec![("Exif", "ISO", "100")]),
  ];

  sort_images(
    &mut images,
    &SortSpec {
      field: SortField::Metadata("ISO".to_string()),
      direction: SortDirection::Asc,
    },
  );

  assert_eq!(images[0].file_name, "low.jpg");
  assert_eq!(images[1].file_name, "high.jpg");
}

#[test]
fn sorts_by_visible_metadata_label() {
  let mut images = vec![
    item("short.jpg", vec![("Exif", "ExposureTime", "1/30 s")]),
    item("long.jpg", vec![("Exif", "ExposureTime", "1/2 s")]),
  ];

  sort_images(
    &mut images,
    &SortSpec {
      field: SortField::Metadata("Exif.ExposureTime".to_string()),
      direction: SortDirection::Desc,
    },
  );

  assert_eq!(images[0].file_name, "long.jpg");
  assert_eq!(images[1].file_name, "short.jpg");
}
