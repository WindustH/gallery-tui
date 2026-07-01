use std::{fmt::Write as FmtWrite, io::Write as IoWrite, path::Path, sync::Arc};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use fast_image_resize::{
  FilterType as FirFilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
  images::{Image as FirImage, ImageRef as FirImageRef},
};
use image::{
  DynamicImage, ExtendedColorType, GrayAlphaImage, GrayImage, ImageBuffer, ImageDecoder,
  ImageEncoder, ImageReader, RgbImage, RgbaImage,
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
use rayon::prelude::*;

use crate::capability::RenderMode;

#[derive(Debug, Clone)]
pub struct NativeImageConfig {
  pub cell_pixels: Option<(u16, u16)>,
  pub passthrough: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedNativeImage {
  image: Arc<DynamicImage>,
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
  let prepared = prepare(path, width_cells, height_cells, config.cell_pixels).await?;
  render_prepared(&prepared, mode, config, image_id).await
}

pub async fn prepare(
  path: &Path,
  width_cells: u16,
  height_cells: u16,
  cell_pixels: Option<(u16, u16)>,
) -> Result<PreparedNativeImage> {
  let image = scale_to_fit(path, width_cells, height_cells, cell_pixels).await?;
  Ok(PreparedNativeImage {
    image: Arc::new(image),
  })
}

pub async fn render_prepared(
  prepared: &PreparedNativeImage,
  mode: RenderMode,
  config: &NativeImageConfig,
  image_id: Option<u32>,
) -> Result<Vec<u8>> {
  let image = prepared.image.clone();
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
    let decoded = decode_image(&path)?;
    let (cell_width, cell_height) = cell_pixels.unwrap_or((8, 16));
    let max_width = u32::from(width_cells.max(1)) * u32::from(cell_width.max(1));
    let max_height = u32::from(height_cells.max(1)) * u32::from(cell_height.max(1));
    let (max_width, max_height) = flip_size(decoded.orientation, (max_width, max_height));
    let (target_width, target_height) = fit_pixel_size(decoded.size, (max_width, max_height));

    let mut image = decoded.image;
    if image.width() != target_width || image.height() != target_height {
      let filter = if target_width > image.width() || target_height > image.height() {
        FilterType::CatmullRom
      } else {
        FilterType::Triangle
      };
      image = resize_exact_fast(image, target_width, target_height, filter);
    }
    if decoded.orientation != Orientation::NoTransforms {
      image.apply_orientation(decoded.orientation);
    }
    Ok(image)
  })
  .await?
}

struct DecodedImage {
  image: DynamicImage,
  orientation: Orientation,
  size: (u32, u32),
}

fn fit_pixel_size(image_size: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
  let (image_width, image_height) = (image_size.0.max(1), image_size.1.max(1));
  let (max_width, max_height) = (bounds.0.max(1), bounds.1.max(1));
  let scale = (max_width as f64 / image_width as f64).min(max_height as f64 / image_height as f64);
  let target_width = ((image_width as f64 * scale).round() as u32).clamp(1, max_width);
  let target_height = ((image_height as f64 * scale).round() as u32).clamp(1, max_height);
  (target_width, target_height)
}

fn decode_image(path: &Path) -> Result<DecodedImage> {
  let reader = ImageReader::open(path)
    .with_context(|| format!("failed to open {}", path.display()))?
    .with_guessed_format()
    .with_context(|| format!("failed to guess image format for {}", path.display()))?;
  let mut decoder = reader
    .into_decoder()
    .with_context(|| format!("failed to decode {}", path.display()))?;
  let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
  let size = decoder.dimensions();
  let image = DynamicImage::from_decoder(decoder)
    .with_context(|| format!("failed to read image pixels from {}", path.display()))?;
  Ok(DecodedImage {
    image,
    orientation,
    size,
  })
}

fn flip_size(orientation: Orientation, size: (u32, u32)) -> (u32, u32) {
  use image::metadata::Orientation::{Rotate90, Rotate90FlipH, Rotate270, Rotate270FlipH};
  match orientation {
    Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH => (size.1, size.0),
    _ => size,
  }
}

fn resize_exact_fast(
  image: DynamicImage,
  target_width: u32,
  target_height: u32,
  filter: FilterType,
) -> DynamicImage {
  match try_resize_exact_fast(&image, target_width, target_height, filter) {
    Ok(resized) => resized,
    Err(_) => image.resize_exact(target_width, target_height, filter),
  }
}

fn try_resize_exact_fast(
  image: &DynamicImage,
  target_width: u32,
  target_height: u32,
  filter: FilterType,
) -> Result<DynamicImage> {
  match image {
    DynamicImage::ImageLuma8(image) => {
      let pixels = resize_u8_pixels(
        image.as_raw(),
        image.width(),
        image.height(),
        target_width,
        target_height,
        PixelType::U8,
        filter,
      )?;
      let image: GrayImage = ImageBuffer::from_raw(target_width, target_height, pixels)
        .context("resized luma buffer has an invalid size")?;
      Ok(DynamicImage::ImageLuma8(image))
    }
    DynamicImage::ImageLumaA8(image) => {
      let pixels = resize_u8_pixels(
        image.as_raw(),
        image.width(),
        image.height(),
        target_width,
        target_height,
        PixelType::U8x2,
        filter,
      )?;
      let image: GrayAlphaImage = ImageBuffer::from_raw(target_width, target_height, pixels)
        .context("resized luma-alpha buffer has an invalid size")?;
      Ok(DynamicImage::ImageLumaA8(image))
    }
    DynamicImage::ImageRgb8(image) => {
      let pixels = resize_u8_pixels(
        image.as_raw(),
        image.width(),
        image.height(),
        target_width,
        target_height,
        PixelType::U8x3,
        filter,
      )?;
      let image: RgbImage = ImageBuffer::from_raw(target_width, target_height, pixels)
        .context("resized rgb buffer has an invalid size")?;
      Ok(DynamicImage::ImageRgb8(image))
    }
    DynamicImage::ImageRgba8(image) => {
      let pixels = resize_u8_pixels(
        image.as_raw(),
        image.width(),
        image.height(),
        target_width,
        target_height,
        PixelType::U8x4,
        filter,
      )?;
      let image: RgbaImage = ImageBuffer::from_raw(target_width, target_height, pixels)
        .context("resized rgba buffer has an invalid size")?;
      Ok(DynamicImage::ImageRgba8(image))
    }
    _ => bail!("fast resize supports only 8-bit native image buffers"),
  }
}

fn resize_u8_pixels(
  pixels: &[u8],
  width: u32,
  height: u32,
  target_width: u32,
  target_height: u32,
  pixel_type: PixelType,
  filter: FilterType,
) -> Result<Vec<u8>> {
  let src = FirImageRef::new(width, height, pixels, pixel_type)?;
  let mut dst = FirImage::new(target_width, target_height, pixel_type);
  let options = ResizeOptions::new().resize_alg(resize_algorithm(filter));
  Resizer::new().resize(&src, &mut dst, Some(&options))?;
  Ok(dst.into_vec())
}

fn resize_algorithm(filter: FilterType) -> ResizeAlg {
  match filter {
    FilterType::Nearest => ResizeAlg::Nearest,
    FilterType::Triangle => ResizeAlg::Convolution(FirFilterType::Bilinear),
    FilterType::CatmullRom => ResizeAlg::Convolution(FirFilterType::CatmullRom),
    FilterType::Gaussian => ResizeAlg::Convolution(FirFilterType::Gaussian),
    FilterType::Lanczos3 => ResizeAlg::Convolution(FirFilterType::Lanczos3),
  }
}

async fn encode_kitty(
  image: Arc<DynamicImage>,
  envelope: &ProtocolEnvelope,
  image_id: u32,
) -> Result<Vec<u8>> {
  let envelope = envelope.clone();
  tokio::task::spawn_blocking(move || {
    let size = (image.width(), image.height());
    match image.as_ref() {
      DynamicImage::ImageRgb8(image) => {
        encode_kitty_raw(image.as_raw(), 24, size, image_id, &envelope)
      }
      DynamicImage::ImageRgba8(image) => {
        encode_kitty_raw(image.as_raw(), 32, size, image_id, &envelope)
      }
      image if image.color().has_alpha() => {
        let image = image.to_rgba8();
        encode_kitty_raw(image.as_raw(), 32, size, image_id, &envelope)
      }
      image => {
        let image = image.to_rgb8();
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
  const RAW_CHUNK_SIZE: usize = 3072;

  let encoded_len = raw.len().div_ceil(3) * 4;
  let chunk_count = raw.len().div_ceil(RAW_CHUNK_SIZE);
  let mut chunks = raw.chunks(RAW_CHUNK_SIZE).peekable();
  let mut encoded = String::with_capacity(4096);
  let mut out = Vec::with_capacity(encoded_len + chunk_count * 64 + 64);
  if let Some(first) = chunks.next() {
    STANDARD.encode_string(first, &mut encoded);
    write!(
      out,
      "{}_Gq=2,a=T,z=-1,C=1,f={format},s={},v={},i={image_id},m={};{}{}\\{}",
      envelope.start,
      size.0,
      size.1,
      u8::from(chunks.peek().is_some()),
      encoded,
      envelope.escape,
      envelope.close
    )?;
  }

  while let Some(chunk) = chunks.next() {
    encoded.clear();
    STANDARD.encode_string(chunk, &mut encoded);
    write!(
      out,
      "{}_Gm={};{}{}\\{}",
      envelope.start,
      u8::from(chunks.peek().is_some()),
      encoded,
      envelope.escape,
      envelope.close
    )?;
  }

  Ok(out)
}

async fn encode_iterm(image: Arc<DynamicImage>, envelope: &ProtocolEnvelope) -> Result<Vec<u8>> {
  let envelope = envelope.clone();
  tokio::task::spawn_blocking(move || {
    let (width, height) = (image.width(), image.height());
    let mut image_bytes = Vec::new();
    if image.color().has_alpha() {
      match image.as_ref() {
        DynamicImage::ImageRgba8(rgba) => {
          PngEncoder::new(&mut image_bytes).write_image(
            rgba.as_raw(),
            width,
            height,
            ExtendedColorType::Rgba8,
          )?;
        }
        image => {
          let rgba = image.to_rgba8();
          PngEncoder::new(&mut image_bytes).write_image(
            rgba.as_raw(),
            width,
            height,
            ExtendedColorType::Rgba8,
          )?;
        }
      }
    } else {
      JpegEncoder::new_with_quality(&mut image_bytes, 85).encode_image(image.as_ref())?;
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

struct SixelPixels {
  rgb: Vec<u8>,
  alpha: Option<Vec<u8>>,
  width: u32,
  height: u32,
}

async fn encode_sixel(image: Arc<DynamicImage>, envelope: &ProtocolEnvelope) -> Result<Vec<u8>> {
  let envelope = envelope.clone();
  tokio::task::spawn_blocking(move || {
    if image.width() == 0 || image.height() == 0 {
      bail!("image is empty");
    }
    let pixels = prepare_sixel_pixels(image.as_ref());
    let has_alpha = pixels.alpha.is_some();
    let quantized = quantify(&pixels.rgb, has_alpha, pixels.width, pixels.height)?;
    let indexed = build_sixel_indices(quantized.indices, pixels.alpha.as_deref());

    let mut out = Vec::new();
    write!(
      out,
      "{}P9;1q\"1;1;{};{}",
      envelope.start, pixels.width, pixels.height
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

    for row in encode_sixel_rows(&indexed, pixels.width, pixels.height)? {
      out.extend(row);
    }

    write!(out, "{}\\{}", envelope.escape, envelope.close)?;
    Ok(out)
  })
  .await?
}

fn prepare_sixel_pixels(image: &DynamicImage) -> SixelPixels {
  let (width, height) = (image.width(), image.height());
  if image.color().has_alpha() {
    match image {
      DynamicImage::ImageRgba8(rgba) => {
        let raw = rgba.as_raw();
        let rgb = raw
          .par_chunks_exact(4)
          .flat_map_iter(|pixel| [pixel[0], pixel[1], pixel[2]])
          .collect();
        let alpha = raw.par_chunks_exact(4).map(|pixel| pixel[3]).collect();
        return SixelPixels {
          rgb,
          alpha: Some(alpha),
          width,
          height,
        };
      }
      image => {
        let rgba = image.to_rgba8();
        let raw = rgba.as_raw();
        let rgb = raw
          .par_chunks_exact(4)
          .flat_map_iter(|pixel| [pixel[0], pixel[1], pixel[2]])
          .collect();
        let alpha = raw.par_chunks_exact(4).map(|pixel| pixel[3]).collect();
        return SixelPixels {
          rgb,
          alpha: Some(alpha),
          width,
          height,
        };
      }
    }
  }

  let rgb = match image {
    DynamicImage::ImageRgb8(rgb) => rgb.as_raw().clone(),
    image => image.to_rgb8().into_raw(),
  };
  SixelPixels {
    rgb,
    alpha: None,
    width,
    height,
  }
}

fn build_sixel_indices(indices: Vec<u8>, alpha: Option<&[u8]>) -> Vec<u8> {
  match alpha {
    Some(alpha) => indices
      .par_iter()
      .zip(alpha.par_iter())
      .map(|(&index, &alpha)| {
        if alpha == 0 {
          0
        } else {
          index.saturating_add(1)
        }
      })
      .collect(),
    None => indices,
  }
}

fn encode_sixel_rows(indexed: &[u8], width: u32, height: u32) -> Result<Vec<Vec<u8>>> {
  let width = width as usize;
  let height = height as usize;
  let rows: Vec<Result<Vec<u8>>> = (0..height)
    .into_par_iter()
    .map(|y| {
      let sixel_char = (b'?' + (1_u8 << (y % 6))) as char;
      let mut out = Vec::new();
      let mut last = 0_u8;
      let mut repeat = 0_usize;

      for &index in &indexed[y * width..(y + 1) * width] {
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
      Ok(out)
    })
    .collect();

  rows.into_iter().collect()
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
