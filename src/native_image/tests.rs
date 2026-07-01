use super::*;
use std::{
  path::PathBuf,
  time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn fit_pixel_size_upscales_to_available_space() {
  assert_eq!(fit_pixel_size((16, 16), (160, 80)), (80, 80));
}

#[test]
fn fit_pixel_size_downscales_to_available_space() {
  assert_eq!(fit_pixel_size((800, 400), (200, 200)), (200, 100));
}

#[test]
fn kitty_erase_can_target_single_image_id() {
  let erase = erase_sequence(RenderMode::Kitty, None, Some(42)).unwrap();

  assert!(erase.contains("a=d,d=i,i=42"));
}

#[test]
fn kitty_erase_can_reset_all_images() {
  let erase = erase_sequence(RenderMode::Kitty, None, None).unwrap();

  assert!(erase.contains("a=d,d=A"));
}

#[test]
fn sixel_alpha_indices_keep_zero_for_transparent_pixels() {
  let alpha = [255, 0, 128];

  assert_eq!(
    build_sixel_indices(vec![2, 3, 4], Some(&alpha)),
    vec![3, 0, 5]
  );
}

#[test]
fn sixel_rows_are_encoded_in_scanline_order() {
  let rows = encode_sixel_rows(&[1, 1, 2, 2, 3, 3, 4, 4], 2, 4).unwrap();
  let encoded = String::from_utf8(rows.concat()).unwrap();

  assert_eq!(encoded, "#1!2@$#2!2A$#3!2C$#4!2G$");
}

#[tokio::test]
async fn prepared_image_can_be_reused_for_native_protocols() {
  let path = unique_temp_path("prepared-native-protocols").with_extension("png");
  let image = RgbaImage::from_fn(4, 4, |x, y| {
    image::Rgba([x as u8 * 40, y as u8 * 40, 180, 255])
  });
  image.save(&path).unwrap();

  let config = NativeImageConfig {
    cell_pixels: Some((1, 1)),
    passthrough: None,
  };
  let prepared = prepare(&path, 2, 2, config.cell_pixels).await.unwrap();
  let kitty = render_prepared(&prepared, RenderMode::Kitty, &config, Some(7))
    .await
    .unwrap();
  let sixel = render_prepared(&prepared, RenderMode::Sixel, &config, None)
    .await
    .unwrap();

  let _ = std::fs::remove_file(path);

  assert!(String::from_utf8_lossy(&kitty).contains("_Gq=2,a=T"));
  assert!(String::from_utf8_lossy(&sixel).contains("P9;1q"));
}

fn unique_temp_path(name: &str) -> PathBuf {
  let nanos = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  std::env::temp_dir().join(format!("gallery-tui-{name}-{}-{nanos}", std::process::id()))
}
