use std::{fmt::Write as FmtWrite, io::Write as IoWrite, path::Path, str};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{
  DynamicImage, ExtendedColorType, GenericImageView, ImageDecoder, ImageEncoder, ImageReader,
  codecs::{jpeg::JpegEncoder, png::PngEncoder},
  imageops::FilterType,
  metadata::Orientation,
};
use palette::{Srgb, cast::ComponentsAs};
use quantette::{
  PaletteSize,
  color_map::IndexedColorMap,
  wu::{BinnerU8x3, WuU8x3},
};

use crate::capability::RenderMode;

#[derive(Debug, Clone)]
pub struct NativeImageConfig {
  pub cell_pixels: Option<(u16, u16)>,
  pub passthrough: Option<String>,
}

#[derive(Debug, Clone)]
struct ProtocolEnvelope {
  start: &'static str,
  escape: &'static str,
  close: &'static str,
}

impl ProtocolEnvelope {
  fn new(passthrough: Option<&str>) -> Self {
    match passthrough {
      Some("tmux") => Self {
        start: "\x1bPtmux;\x1b\x1b",
        escape: "\x1b\x1b",
        close: "\x1b\\",
      },
      _ => Self {
        start: "\x1b",
        escape: "\x1b",
        close: "",
      },
    }
  }
}

pub async fn render(
  path: &Path,
  width_cells: u16,
  height_cells: u16,
  mode: RenderMode,
  config: &NativeImageConfig,
  image_id: Option<u32>,
) -> Result<Vec<u8>> {
  let image = scale_to_fit(path, width_cells, height_cells, config.cell_pixels).await?;
  let envelope = ProtocolEnvelope::new(config.passthrough.as_deref());
  match mode {
    RenderMode::Kitty => encode_kitty(image, &envelope, image_id.unwrap_or(1)).await,
    RenderMode::Iterm2 => encode_iterm(image, &envelope).await,
    RenderMode::Sixel => encode_sixel(image, &envelope).await,
    RenderMode::Symbols | RenderMode::Ascii => bail!("{} is not a native image mode", mode.label()),
  }
}

pub fn erase_sequence(
  mode: RenderMode,
  passthrough: Option<&str>,
  image_id: Option<u32>,
) -> Option<String> {
  if mode != RenderMode::Kitty {
    return None;
  }
  let envelope = ProtocolEnvelope::new(passthrough);
  Some(match image_id {
    Some(image_id) => format!(
      "{}_Gq=2,a=d,d=i,i={image_id}{}\\{}",
      envelope.start, envelope.escape, envelope.close
    ),
    None => format!(
      "{}_Gq=2,a=d,d=A{}\\{}",
      envelope.start, envelope.escape, envelope.close
    ),
  })
}

async fn scale_to_fit(
  path: &Path,
  width_cells: u16,
  height_cells: u16,
  cell_pixels: Option<(u16, u16)>,
) -> Result<DynamicImage> {
  let path = path.to_path_buf();
  tokio::task::spawn_blocking(move || {
    let (mut image, orientation) = decode_image(&path)?;
    let (cell_width, cell_height) = cell_pixels.unwrap_or((8, 16));
    let max_width = u32::from(width_cells.max(1)) * u32::from(cell_width.max(1));
    let max_height = u32::from(height_cells.max(1)) * u32::from(cell_height.max(1));
    let (max_width, max_height) = flip_size(orientation, (max_width, max_height));
    let (target_width, target_height) =
      fit_pixel_size((image.width(), image.height()), (max_width, max_height));

    if image.width() != target_width || image.height() != target_height {
      let filter = if target_width > image.width() || target_height > image.height() {
        FilterType::CatmullRom
      } else {
        FilterType::Triangle
      };
      image = image.resize_exact(target_width, target_height, filter);
    }
    if orientation != Orientation::NoTransforms {
      image.apply_orientation(orientation);
    }
    Ok(image)
  })
  .await?
}

fn fit_pixel_size(image_size: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
  let (image_width, image_height) = (image_size.0.max(1), image_size.1.max(1));
  let (max_width, max_height) = (bounds.0.max(1), bounds.1.max(1));
  let scale = (max_width as f64 / image_width as f64).min(max_height as f64 / image_height as f64);
  let target_width = ((image_width as f64 * scale).round() as u32).clamp(1, max_width);
  let target_height = ((image_height as f64 * scale).round() as u32).clamp(1, max_height);
  (target_width, target_height)
}

fn decode_image(path: &Path) -> Result<(DynamicImage, Orientation)> {
  let reader = ImageReader::open(path)
    .with_context(|| format!("failed to open {}", path.display()))?
    .with_guessed_format()
    .with_context(|| format!("failed to guess image format for {}", path.display()))?;
  let mut decoder = reader
    .into_decoder()
    .with_context(|| format!("failed to decode {}", path.display()))?;
  let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
  let image = DynamicImage::from_decoder(decoder)
    .with_context(|| format!("failed to read image pixels from {}", path.display()))?;
  Ok((image, orientation))
}

fn flip_size(orientation: Orientation, size: (u32, u32)) -> (u32, u32) {
  use image::metadata::Orientation::{Rotate90, Rotate90FlipH, Rotate270, Rotate270FlipH};
  match orientation {
    Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH => (size.1, size.0),
    _ => size,
  }
}

async fn encode_kitty(
  image: DynamicImage,
  envelope: &ProtocolEnvelope,
  image_id: u32,
) -> Result<Vec<u8>> {
  let envelope = envelope.clone();
  tokio::task::spawn_blocking(move || {
    let size = (image.width(), image.height());
    match image {
      DynamicImage::ImageRgb8(image) => {
        encode_kitty_raw(image.as_raw(), 24, size, image_id, &envelope)
      }
      DynamicImage::ImageRgba8(image) => {
        encode_kitty_raw(image.as_raw(), 32, size, image_id, &envelope)
      }
      image if image.color().has_alpha() => {
        let image = image.into_rgba8();
        encode_kitty_raw(image.as_raw(), 32, size, image_id, &envelope)
      }
      image => {
        let image = image.into_rgb8();
        encode_kitty_raw(image.as_raw(), 24, size, image_id, &envelope)
      }
    }
  })
  .await?
}

fn encode_kitty_raw(
  raw: &[u8],
  format: u8,
  size: (u32, u32),
  image_id: u32,
  envelope: &ProtocolEnvelope,
) -> Result<Vec<u8>> {
  let encoded = STANDARD.encode(raw).into_bytes();
  let mut chunks = encoded.chunks(4096).peekable();
  let mut out = Vec::with_capacity(encoded.len() + chunks.len() * 64 + 64);
  if let Some(first) = chunks.next() {
    write!(
      out,
      "{}_Gq=2,a=T,z=-1,C=1,f={format},s={},v={},i={image_id},m={};{}{}\\{}",
      envelope.start,
      size.0,
      size.1,
      u8::from(chunks.peek().is_some()),
      str::from_utf8(first)?,
      envelope.escape,
      envelope.close
    )?;
  }

  while let Some(chunk) = chunks.next() {
    write!(
      out,
      "{}_Gm={};{}{}\\{}",
      envelope.start,
      u8::from(chunks.peek().is_some()),
      str::from_utf8(chunk)?,
      envelope.escape,
      envelope.close
    )?;
  }

  Ok(out)
}

async fn encode_iterm(image: DynamicImage, envelope: &ProtocolEnvelope) -> Result<Vec<u8>> {
  let envelope = envelope.clone();
  tokio::task::spawn_blocking(move || {
    let (width, height) = (image.width(), image.height());
    let mut image_bytes = Vec::new();
    if image.color().has_alpha() {
      let rgba = image.into_rgba8();
      PngEncoder::new(&mut image_bytes).write_image(
        rgba.as_raw(),
        width,
        height,
        ExtendedColorType::Rgba8,
      )?;
    } else {
      JpegEncoder::new_with_quality(&mut image_bytes, 85).encode_image(&image)?;
    }

    let mut out = String::with_capacity(256 + image_bytes.len() * 4 / 3);
    write!(
      out,
      "{}]1337;File=inline=1;size={};width={width}px;height={height}px;doNotMoveCursor=1:",
      envelope.start,
      image_bytes.len()
    )?;
    STANDARD.encode_string(image_bytes, &mut out);
    write!(out, "\x07{}", envelope.close)?;
    Ok(out.into_bytes())
  })
  .await?
}

struct QuantizeOutput<T> {
  indices: Vec<u8>,
  palette: Vec<T>,
}

async fn encode_sixel(image: DynamicImage, envelope: &ProtocolEnvelope) -> Result<Vec<u8>> {
  let envelope = envelope.clone();
  tokio::task::spawn_blocking(move || {
    if image.width() == 0 || image.height() == 0 {
      bail!("image is empty");
    }
    let has_alpha = image.color().has_alpha();
    let quantized = match &image {
      DynamicImage::ImageRgb8(rgb) => quantify(rgb.as_raw(), false, rgb.width(), rgb.height())?,
      _ => {
        let rgb = image.to_rgb8();
        quantify(rgb.as_raw(), has_alpha, rgb.width(), rgb.height())?
      }
    };

    let mut out = Vec::new();
    write!(
      out,
      "{}P9;1q\"1;1;{};{}",
      envelope.start,
      image.width(),
      image.height()
    )?;

    for (index, color) in quantized.palette.iter().enumerate() {
      write!(
        out,
        "#{};2;{};{};{}",
        index + usize::from(has_alpha),
        u16::from(color.red) * 100 / 255,
        u16::from(color.green) * 100 / 255,
        u16::from(color.blue) * 100 / 255
      )?;
    }

    for y in 0..image.height() {
      let sixel_char = (b'?' + (1 << (y % 6))) as char;
      let mut last = 0_u8;
      let mut repeat = 0_usize;

      for x in 0..image.width() {
        let pixel = image.get_pixel(x, y);
        let transparent = has_alpha && pixel.0[3] == 0;
        let index = if transparent {
          0
        } else {
          quantized.indices[(y * image.width() + x) as usize] + u8::from(has_alpha)
        };

        if index == last || repeat == 0 {
          last = index;
          repeat += 1;
          continue;
        }
        write_sixel_run(&mut out, last, repeat, sixel_char)?;
        last = index;
        repeat = 1;
      }

      write_sixel_run(&mut out, last, repeat, sixel_char)?;
      write!(out, "$")?;
      if y % 6 == 5 {
        write!(out, "-")?;
      }
    }

    write!(out, "{}\\{}", envelope.escape, envelope.close)?;
    Ok(out)
  })
  .await?
}

#[cfg(test)]
mod tests {
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
}

fn quantify(
  rgb: &[u8],
  has_alpha: bool,
  width: u32,
  height: u32,
) -> Result<QuantizeOutput<Srgb<u8>>> {
  let color_count = width as usize * height as usize;
  let colors: &[Srgb<u8>] = rgb[..color_count * 3].components_as();
  let palette_size = PaletteSize::try_from(256_u16 - u16::from(has_alpha))?;
  let color_map = WuU8x3::run_slice(colors, BinnerU8x3::rgb())?.color_map(palette_size);
  Ok(QuantizeOutput {
    indices: color_map.map_to_indices(colors),
    palette: color_map.into_palette().into_vec(),
  })
}

fn write_sixel_run(out: &mut Vec<u8>, index: u8, repeat: usize, sixel_char: char) -> Result<()> {
  if repeat > 1 {
    write!(out, "#{index}!{repeat}{sixel_char}")?;
  } else {
    write!(out, "#{index}{sixel_char}")?;
  }
  Ok(())
}
