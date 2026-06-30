use ratatui::layout::Rect;

use crate::{config::EffectiveLayoutConfig, model::ImageItem};

#[derive(Debug, Clone, Copy)]
pub struct CanvasRect {
  pub x: u16,
  pub y: u32,
  pub width: u16,
  pub height: u16,
}

impl CanvasRect {
  pub fn center_x(self) -> i32 {
    i32::from(self.x) + i32::from(self.width / 2)
  }
}

#[derive(Debug, Clone)]
pub struct BrowserLayout {
  pub cards: Vec<CanvasRect>,
  pub total_height: u32,
  pub columns: usize,
}

pub fn compute_browser_layout(
  items: &[ImageItem],
  area: Rect,
  config: &EffectiveLayoutConfig,
) -> BrowserLayout {
  match config.strategy.as_str() {
    "masonry" => masonry_layout(items, area.width, config),
    "list" => list_layout(items.len(), area.width, area.height, config),
    _ => fixed_grid_layout(items.len(), area.width, area.height, config),
  }
}

pub fn screen_rect(card: CanvasRect, viewport: Rect, scroll: u32) -> Option<Rect> {
  let top = card.y as i64 - scroll as i64 + i64::from(viewport.y);
  let bottom = top + i64::from(card.height);
  let viewport_top = i64::from(viewport.y);
  let viewport_bottom = i64::from(viewport.y + viewport.height);
  if bottom <= viewport_top || top >= viewport_bottom {
    return None;
  }
  let visible_top = top.max(viewport_top);
  let visible_bottom = bottom.min(viewport_bottom);
  let visible_height = visible_bottom.saturating_sub(visible_top);
  if visible_height == 0 {
    return None;
  }

  Some(Rect {
    x: viewport.x.saturating_add(card.x),
    y: visible_top as u16,
    width: card.width.min(viewport.width.saturating_sub(card.x)),
    height: visible_height as u16,
  })
}

fn fixed_grid_layout(
  count: usize,
  area_width: u16,
  area_height: u16,
  config: &EffectiveLayoutConfig,
) -> BrowserLayout {
  let columns = config.columns.max(1) as usize;
  let rows = config.rows.max(1) as usize;
  let card_width = fit_slot(area_width, columns, config.gap_x).max(1);
  let card_height = fit_slot(area_height, rows, config.gap_y).max(1);
  let x_offset = centered_offset(area_width, columns, card_width, config.gap_x);
  let mut cards = Vec::with_capacity(count);
  for index in 0..count {
    let row = index / columns;
    let col = index % columns;
    cards.push(CanvasRect {
      x: x_offset
        .saturating_add((col as u16).saturating_mul(card_width.saturating_add(config.gap_x))),
      y: (row as u32).saturating_mul(u32::from(card_height.saturating_add(config.gap_y))),
      width: card_width,
      height: card_height,
    });
  }
  let rows = count.div_ceil(columns);
  let total_height = (rows as u32)
    .saturating_mul(u32::from(card_height.saturating_add(config.gap_y)))
    .saturating_sub(u32::from(config.gap_y));
  BrowserLayout {
    cards,
    total_height,
    columns,
  }
}

fn list_layout(
  count: usize,
  area_width: u16,
  area_height: u16,
  config: &EffectiveLayoutConfig,
) -> BrowserLayout {
  let items_per_page = config.items.max(1) as usize;
  let card_height = fit_slot(area_height, items_per_page, config.gap_y).max(1);
  let mut cards = Vec::with_capacity(count);
  for index in 0..count {
    cards.push(CanvasRect {
      x: 0,
      y: (index as u32).saturating_mul(u32::from(card_height.saturating_add(config.gap_y))),
      width: area_width.max(1),
      height: card_height,
    });
  }
  let total_height = (count as u32)
    .saturating_mul(u32::from(card_height.saturating_add(config.gap_y)))
    .saturating_sub(u32::from(config.gap_y));
  BrowserLayout {
    cards,
    total_height,
    columns: 1,
  }
}

fn masonry_layout(
  items: &[ImageItem],
  area_width: u16,
  config: &EffectiveLayoutConfig,
) -> BrowserLayout {
  let configured_columns = config.columns;
  let card_width = if configured_columns > 0 {
    fit_slot(area_width, configured_columns as usize, config.gap_x).max(1)
  } else {
    config.card_width.max(1).min(area_width.max(1))
  };
  let columns = columns_for_width(area_width, card_width, config.gap_x, configured_columns);
  let x_offset = centered_offset(area_width, columns, card_width, config.gap_x);
  let mut heights = vec![0_u32; columns];
  let mut cards = Vec::with_capacity(items.len());

  for item in items {
    let (col, current_y) = heights
      .iter()
      .enumerate()
      .min_by_key(|(_, height)| *height)
      .map(|(col, height)| (col, *height))
      .unwrap_or((0, 0));
    let height = masonry_card_height(item, card_width, config);
    cards.push(CanvasRect {
      x: x_offset
        .saturating_add((col as u16).saturating_mul(card_width.saturating_add(config.gap_x))),
      y: current_y,
      width: card_width,
      height,
    });
    heights[col] = current_y
      .saturating_add(u32::from(height))
      .saturating_add(u32::from(config.gap_y));
  }

  let total_height = heights.into_iter().max().unwrap_or(0);
  BrowserLayout {
    cards,
    total_height,
    columns,
  }
}

fn masonry_card_height(item: &ImageItem, card_width: u16, config: &EffectiveLayoutConfig) -> u16 {
  let label_extra = if config.show_filename
    && config.card_style != "image_only"
    && matches!(config.filename_position.as_str(), "top" | "bottom")
  {
    config.label_lines.max(1)
  } else {
    0
  };
  let border_extra = if config.show_border { 2 } else { 0 };
  let image_height = item
    .dimensions
    .map(|(w, h)| {
      let ratio = h as f32 / w.max(1) as f32;
      // Terminal cells are taller than they are wide; halve the pixel aspect ratio.
      (f32::from(card_width) * ratio * 0.5).round() as u16
    })
    .unwrap_or(config.card_height.saturating_sub(label_extra));
  image_height
    .saturating_add(label_extra)
    .saturating_add(border_extra)
    .clamp(4, 80)
}

fn columns_for_width(area_width: u16, card_width: u16, gap: u16, configured: u16) -> usize {
  if configured > 0 {
    return configured as usize;
  }
  let stride = card_width.saturating_add(gap).max(1);
  ((area_width.saturating_add(gap)) / stride).max(1) as usize
}

fn fit_slot(total: u16, count: usize, gap: u16) -> u16 {
  if count == 0 {
    return total.max(1);
  }
  let gaps = gap.saturating_mul(count.saturating_sub(1) as u16);
  total
    .saturating_sub(gaps)
    .checked_div(count as u16)
    .unwrap_or(1)
}

fn centered_offset(total: u16, count: usize, item: u16, gap: u16) -> u16 {
  if count == 0 {
    return 0;
  }
  let used = (count as u16)
    .saturating_mul(item)
    .saturating_add(gap.saturating_mul(count.saturating_sub(1) as u16));
  total.saturating_sub(used) / 2
}

#[cfg(test)]
#[path = "layout/tests.rs"]
mod tests;
