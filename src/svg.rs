//! SVG support: rasterize vector files to PNG with resvg so the rest of
//! the pipeline (protocol graphics, Chafa, the render cache) sees a plain
//! raster image.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Whether `path` refers to an SVG (by extension, lowercase).
pub(crate) fn is_svg(path: &Path) -> bool {
  path
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

/// Parse an SVG and return its intrinsic size in pixels.
///
/// usvg always produces a concrete size (falling back to 100% of the
/// viewBox), so this only fails when the file cannot be parsed.
pub(crate) fn svg_dimensions(path: &Path) -> Result<(u32, u32)> {
  let text = std::fs::read_to_string(path)
    .with_context(|| format!("reading {}", path.display()))?;
  let tree = parse(&text)?;
  Ok(size_of(&tree))
}

fn parse(text: &str) -> Result<usvg::Tree> {
  usvg::Tree::from_str(text, &usvg::Options::default())
    .map_err(|error| anyhow::anyhow!("parsing SVG: {error}"))
}

fn size_of(tree: &usvg::Tree) -> (u32, u32) {
  let size = tree.size();
  (
    (size.width().round() as u32).max(1),
    (size.height().round() as u32).max(1),
  )
}

/// Rasterize `svg` to fit inside `target` (contain, aspect preserved —
/// SVGs scale up losslessly) and return the cached PNG path. The output
/// is keyed by the SVG path, its mtime and the target box, so edits and
/// resizes re-rasterize while unchanged files reuse the cache.
pub(crate) async fn ensure_rasterized(
  svg: &Path,
  target: (u32, u32),
  cache_dir: &Path,
) -> Result<PathBuf, String> {
  let modified = std::fs::metadata(svg)
    .and_then(|metadata| metadata.modified())
    .ok()
    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|duration| duration.as_nanos())
    .unwrap_or(0);

  let key_input = format!(
    "gallery-tui-svg-v1|{}|{modified}|{}x{}",
    svg.display(),
    target.0,
    target.1,
  );
  let digest = sha256_hex(key_input.as_bytes());
  let dir = cache_dir.join("svg");
  let png_path = dir.join(format!("{}.png", &digest[..32]));

  if png_path.exists() {
    return Ok(png_path);
  }

  let svg = svg.to_path_buf();
  let png_target = png_path.clone();
  let (target_w, target_h) = target;
  let rendered = tokio::task::spawn_blocking(move || {
    let text = std::fs::read_to_string(&svg)
      .with_context(|| format!("reading {}", svg.display()))?;
    let tree = parse(&text)?;
    rasterize(&tree, target_w, target_h)
  })
  .await
  .map_err(|error| format!("svg rasterization task failed: {error}"))?
  .map_err(|error| error.to_string())?;

  std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
  crate::fs_atomic::write(&png_target, rendered)
    .await
    .map_err(|error| format!("writing {}: {error}", png_target.display()))?;
  Ok(png_path)
}

/// Render `tree` scaled to fit inside `target` (contain).
fn rasterize(tree: &usvg::Tree, target_w: u32, target_h: u32) -> Result<Vec<u8>> {
  let (target_w, target_h) = (target_w.max(1), target_h.max(1));
  let size = tree.size();
  let scale = (f64::from(target_w) / f64::from(size.width()))
    .min(f64::from(target_h) / f64::from(size.height()))
    .max(1e-9);
  let width = ((f64::from(size.width()) * scale).round() as u32)
    .clamp(1, target_w)
    .min(8192);
  let height = ((f64::from(size.height()) * scale).round() as u32)
    .clamp(1, target_h)
    .min(8192);

  let scale = f64::from(width) / f64::from(size.width());
  let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
    .context("allocating SVG raster buffer")?;
  resvg::render(
    tree,
    resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32),
    &mut pixmap.as_mut(),
  );
  pixmap.encode_png().context("encoding rasterized SVG as PNG")
}

fn sha256_hex(data: &[u8]) -> String {
  use std::fmt::Write as _;
  let digest = <sha2::Sha256 as sha2::Digest>::digest(data);
  let mut out = String::with_capacity(digest.len() * 2);
  for byte in digest {
    let _ = write!(out, "{byte:02x}");
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="#123456"/></svg>"##;

  #[test]
  fn svg_dimensions_report_intrinsic_size() {
    let tree = parse(SAMPLE_SVG).expect("sample parses");
    assert_eq!(size_of(&tree), (200, 100));
  }

  #[test]
  fn rasterize_fits_target_box() {
    let tree = parse(SAMPLE_SVG).expect("sample parses");
    // Contain within 400x400: the 2:1 image scales to 400x200.
    let png = rasterize(&tree, 400, 400).expect("renders");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    // Decode the IHDR to check the dimensions.
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 400);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 200);
  }

  #[test]
  fn is_svg_matches_extension_case_insensitively() {
    assert!(is_svg(Path::new("a/b/Icon.SVG")));
    assert!(is_svg(Path::new("icon.svg")));
    assert!(!is_svg(Path::new("icon.png")));
    assert!(!is_svg(Path::new("noext")));
  }
}
