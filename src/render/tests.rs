use super::*;

#[tokio::test]
async fn cache_file_round_trips_compressed_payload() {
  let config = RenderConfig::default();
  let payload = b"\x1b[38;2;120;220;180m##\x1b[0m\n".repeat(4096);

  let encoded = encode_cache_file(&payload, 80, 24, None, RenderMode::Symbols, None, &config)
    .await
    .unwrap();
  let header_end = encoded
    .windows(2)
    .position(|window| window == b"\n\n")
    .unwrap();
  let encoded_text = std::str::from_utf8(&encoded[..header_end]).unwrap();

  assert!(encoded.starts_with(CACHE_MAGIC.as_bytes()));
  assert!(encoded_text.contains("compression=zstd"));
  assert!(encoded.len() < payload.len());

  let decoded = decode_cache_file(&encoded, 80, 24, None, RenderMode::Symbols, None)
    .await
    .unwrap();

  assert_eq!(decoded.payload, payload);
  assert!(!decoded.should_rewrite);
}

#[tokio::test]
async fn legacy_raw_cache_is_read_and_marked_for_rewrite() {
  let payload = b"raw ansi payload";
  let mut encoded = format!(
    "{LEGACY_RAW_CACHE_MAGIC}\nwidth=12\nheight=5\ncell_width=0\ncell_height=0\nmode={}\n\n",
    RenderMode::Ascii.label()
  )
  .into_bytes();
  encoded.extend_from_slice(payload);

  let decoded = decode_cache_file(&encoded, 12, 5, None, RenderMode::Ascii, None)
    .await
    .unwrap();

  assert_eq!(decoded.payload, payload);
  assert!(decoded.should_rewrite);
}

#[tokio::test]
async fn protocol_cache_round_trips_image_id() {
  let config = RenderConfig::default();
  let payload = b"\x1b_Gq=2,a=T,i=42;AAAA\x1b\\";

  let encoded = encode_cache_file(payload, 10, 5, None, RenderMode::Kitty, Some(42), &config)
    .await
    .unwrap();
  let decoded = decode_cache_file(&encoded, 10, 5, None, RenderMode::Kitty, Some(42))
    .await
    .unwrap();

  assert_eq!(decoded.payload, payload);
  assert_eq!(decoded.image_id, Some(42));
  assert!(!decoded.should_rewrite);
}
