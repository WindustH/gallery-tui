use super::*;

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
