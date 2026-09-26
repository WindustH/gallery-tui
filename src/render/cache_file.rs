//! On-disk render cache file format.
//!
//! A cache file is a text header (`key=value` lines ending with a blank line)
//! followed by the zstd-compressed payload. Protocol renders that carry a
//! separate refresh sequence use a small framed payload so both parts survive
//! a round trip.

use std::io::{Cursor, Write};

use img_tui::RenderMode;

const CACHE_MAGIC: &str = "gallery-tui-cache-v7";
const LEGACY_RAW_CACHE_MAGIC: &str = "gallery-tui-cache-v4";
const FRAMED_PAYLOAD_MAGIC: &[u8] = b"gallery-tui-rendered-bytes-v1\0";

/// Raw bytes produced by a render backend.
#[derive(Debug, Clone)]
pub(super) struct RenderedBytes {
  pub(super) data: Vec<u8>,
  pub(super) refresh: Option<Vec<u8>>,
}

/// Header fields that must match for a cache file to be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CacheHeader {
  pub(super) width: u16,
  pub(super) height: u16,
  pub(super) cell_pixels: Option<(u16, u16)>,
  pub(super) mode: RenderMode,
  pub(super) image_id: Option<u32>,
  pub(super) placement_id: Option<u32>,
}

pub(super) struct DecodedCacheFile {
  pub(super) payload: RenderedBytes,
  /// The file uses an older layout and should be rewritten.
  pub(super) should_rewrite: bool,
}

pub(super) async fn encode_cache_file(
  payload: &RenderedBytes,
  header: &CacheHeader,
  compression_level: i32,
  compression_threads: u32,
) -> Result<Vec<u8>, String> {
  let payload_format = if payload.refresh.is_some() {
    "framed"
  } else {
    "raw"
  };
  let payload = encode_rendered_bytes(payload)?;
  let plain_len = payload.len();
  let compressed = tokio::task::spawn_blocking(move || {
    compress_zstd(payload, compression_level, compression_threads)
  })
  .await
  .map_err(|err| format!("zstd compression worker failed: {err}"))?
  .map_err(|err| format!("zstd compression failed: {err}"))?;

  let (cell_width, cell_height) = header.cell_pixels.unwrap_or((0, 0));
  let mut text = format!(
    "{CACHE_MAGIC}\nwidth={}\nheight={}\ncell_width={cell_width}\ncell_height={cell_height}\nmode={}\ncompression=zstd\npayload_format={payload_format}\nuncompressed_bytes={plain_len}\n",
    header.width,
    header.height,
    header.mode.label()
  );
  if let Some(image_id) = header.image_id {
    text.push_str(&format!("image_id={image_id}\n"));
  }
  if let Some(placement_id) = header.placement_id {
    text.push_str(&format!("placement_id={placement_id}\n"));
  }
  text.push('\n');
  let mut out = Vec::with_capacity(text.len() + compressed.len());
  out.extend_from_slice(text.as_bytes());
  out.extend_from_slice(&compressed);
  Ok(out)
}

/// Parse a cache file and check it against `expected`. Any mismatch is an
/// error so the caller falls back to rendering from the source image.
pub(super) async fn decode_cache_file(
  bytes: &[u8],
  expected: &CacheHeader,
) -> Result<DecodedCacheFile, String> {
  let header_end = bytes
    .windows(2)
    .position(|window| window == b"\n\n")
    .ok_or_else(|| "cache metadata header missing".to_string())?;
  let header = std::str::from_utf8(&bytes[..header_end])
    .map_err(|err| format!("cache metadata is not utf-8: {err}"))?;
  let mut lines = header.lines();
  let magic = lines
    .next()
    .ok_or_else(|| "cache metadata magic missing".to_string())?;
  if magic != CACHE_MAGIC && magic != LEGACY_RAW_CACHE_MAGIC {
    return Err("cache metadata magic mismatch".to_string());
  }
  let fields = HeaderFields::parse(lines);
  fields.check(expected)?;

  let payload = &bytes[header_end + 2..];
  let payload_format = fields.payload_format.unwrap_or("raw");
  match fields.compression.unwrap_or("none") {
    "none" => Ok(DecodedCacheFile {
      payload: decode_rendered_bytes(payload.to_vec(), payload_format)?,
      should_rewrite: magic != CACHE_MAGIC || payload_format != "raw",
    }),
    "zstd" => {
      let payload = payload.to_vec();
      let decoded = tokio::task::spawn_blocking(move || decompress_zstd(payload))
        .await
        .map_err(|err| format!("zstd decompression worker failed: {err}"))?
        .map_err(|err| format!("zstd decompression failed: {err}"))?;
      if let Some(expected_len) = fields.uncompressed_bytes
        && decoded.len() != expected_len
      {
        return Err(format!(
          "cache decompressed size mismatch: got {}, expected {}",
          decoded.len(),
          expected_len
        ));
      }
      Ok(DecodedCacheFile {
        payload: decode_rendered_bytes(decoded, payload_format)?,
        should_rewrite: false,
      })
    }
    value => Err(format!("unsupported cache compression: {value}")),
  }
}

#[derive(Default)]
struct HeaderFields<'a> {
  width: Option<u16>,
  height: Option<u16>,
  cell_width: Option<u16>,
  cell_height: Option<u16>,
  mode: Option<&'a str>,
  compression: Option<&'a str>,
  payload_format: Option<&'a str>,
  uncompressed_bytes: Option<usize>,
  image_id: Option<u32>,
  placement_id: Option<u32>,
}

impl<'a> HeaderFields<'a> {
  fn parse(lines: impl Iterator<Item = &'a str>) -> Self {
    let mut fields = Self::default();
    for line in lines {
      let Some((key, value)) = line.split_once('=') else {
        continue;
      };
      match key {
        "width" => fields.width = value.parse().ok(),
        "height" => fields.height = value.parse().ok(),
        "cell_width" => fields.cell_width = value.parse().ok(),
        "cell_height" => fields.cell_height = value.parse().ok(),
        "mode" => fields.mode = Some(value),
        "compression" => fields.compression = Some(value),
        "payload_format" => fields.payload_format = Some(value),
        "uncompressed_bytes" => fields.uncompressed_bytes = value.parse().ok(),
        "image_id" => fields.image_id = value.parse().ok(),
        "placement_id" => fields.placement_id = value.parse().ok(),
        _ => {}
      }
    }
    fields
  }

  fn check(&self, expected: &CacheHeader) -> Result<(), String> {
    if self.width != Some(expected.width) || self.height != Some(expected.height) {
      return Err(format!(
        "cache size mismatch: got {:?}x{:?}, expected {}x{}",
        self.width, self.height, expected.width, expected.height
      ));
    }
    if self.mode != Some(expected.mode.label()) {
      return Err(format!(
        "cache mode mismatch: got {:?}, expected {}",
        self.mode,
        expected.mode.label()
      ));
    }
    let (cell_width, cell_height) = expected.cell_pixels.unwrap_or((0, 0));
    if self.cell_width != Some(cell_width) || self.cell_height != Some(cell_height) {
      return Err(format!(
        "cache cell size mismatch: got {:?}x{:?}, expected {cell_width}x{cell_height}",
        self.cell_width, self.cell_height
      ));
    }
    if self.image_id != expected.image_id {
      return Err(format!(
        "cache image id mismatch: got {:?}, expected {:?}",
        self.image_id, expected.image_id
      ));
    }
    if self.placement_id != expected.placement_id {
      return Err(format!(
        "cache placement id mismatch: got {:?}, expected {:?}",
        self.placement_id, expected.placement_id
      ));
    }
    Ok(())
  }
}

fn compress_zstd(payload: Vec<u8>, level: i32, threads: u32) -> std::io::Result<Vec<u8>> {
  let mut encoder = zstd::stream::Encoder::new(Vec::new(), level)?;
  if threads > 0 {
    encoder.multithread(threads)?;
  }
  encoder.write_all(&payload)?;
  encoder.finish()
}

fn decompress_zstd(payload: Vec<u8>) -> std::io::Result<Vec<u8>> {
  zstd::stream::decode_all(Cursor::new(payload))
}

fn encode_rendered_bytes(payload: &RenderedBytes) -> Result<Vec<u8>, String> {
  let Some(refresh) = &payload.refresh else {
    return Ok(payload.data.clone());
  };
  let data_len = u64::try_from(payload.data.len())
    .map_err(|_| "render payload is too large to cache".to_string())?;
  let refresh_len = u64::try_from(refresh.len())
    .map_err(|_| "refresh payload is too large to cache".to_string())?;
  let mut out = Vec::with_capacity(
    FRAMED_PAYLOAD_MAGIC.len() + 16 + payload.data.len().saturating_add(refresh.len()),
  );
  out.extend_from_slice(FRAMED_PAYLOAD_MAGIC);
  out.extend_from_slice(&data_len.to_le_bytes());
  out.extend_from_slice(&refresh_len.to_le_bytes());
  out.extend_from_slice(&payload.data);
  out.extend_from_slice(refresh);
  Ok(out)
}

fn decode_rendered_bytes(
  mut bytes: Vec<u8>,
  payload_format: &str,
) -> Result<RenderedBytes, String> {
  if payload_format != "framed" {
    return Ok(RenderedBytes {
      data: bytes,
      refresh: None,
    });
  }
  let header_len = FRAMED_PAYLOAD_MAGIC.len() + 16;
  if bytes.len() < header_len || !bytes.starts_with(FRAMED_PAYLOAD_MAGIC) {
    return Err("framed render payload magic mismatch".to_string());
  }
  let lengths = &bytes[FRAMED_PAYLOAD_MAGIC.len()..header_len];
  let read_len = |range: std::ops::Range<usize>, what: &str| -> Result<usize, String> {
    let raw = u64::from_le_bytes(
      lengths[range]
        .try_into()
        .map_err(|_| format!("render payload {what} length missing"))?,
    );
    usize::try_from(raw).map_err(|_| format!("render payload {what} length is too large"))
  };
  let data_len = read_len(0..8, "data")?;
  let refresh_len = read_len(8..16, "refresh")?;
  let refresh_start = header_len.saturating_add(data_len);
  let end = refresh_start.saturating_add(refresh_len);
  if end != bytes.len() {
    return Err(format!(
      "framed render payload size mismatch: got {}, expected {}",
      bytes.len(),
      end
    ));
  }
  let refresh = bytes.split_off(refresh_start);
  bytes.drain(..header_len);
  Ok(RenderedBytes {
    data: bytes,
    refresh: Some(refresh),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn header() -> CacheHeader {
    CacheHeader {
      width: 12,
      height: 7,
      cell_pixels: Some((8, 16)),
      mode: RenderMode::Kitty,
      image_id: Some(42),
      placement_id: Some(42),
    }
  }

  #[tokio::test]
  async fn cache_file_round_trips_framed_payload() {
    let payload = RenderedBytes {
      data: b"upload".to_vec(),
      refresh: Some(b"place".to_vec()),
    };
    let encoded = encode_cache_file(&payload, &header(), 3, 0).await.unwrap();
    let decoded = decode_cache_file(&encoded, &header()).await.unwrap();
    assert_eq!(decoded.payload.data, b"upload");
    assert_eq!(decoded.payload.refresh.as_deref(), Some(&b"place"[..]));
    assert!(!decoded.should_rewrite);
  }

  #[tokio::test]
  async fn cache_file_rejects_mismatched_header() {
    let payload = RenderedBytes {
      data: b"text".to_vec(),
      refresh: None,
    };
    let encoded = encode_cache_file(&payload, &header(), 3, 0).await.unwrap();
    let other = CacheHeader {
      width: 13,
      ..header()
    };
    assert!(decode_cache_file(&encoded, &other).await.is_err());
  }
}
