use ratatui::layout::Rect;

use crate::{config::EffectiveLayoutConfig, model::ImageItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserLayout {
  pub cards: Vec<CanvasRect>,
  pub total_height: u32,
  pub columns: usize,
  bottom_prefix: Vec<u32>,
}

impl BrowserLayout {
  fn new(cards: Vec<CanvasRect>, total_height: u32, columns: usize) -> Self {
    let mut bottom = 0;
    let bottom_prefix = cards
      .iter()
      .map(|card| {
        bottom = bottom.max(card.y.saturating_add(u32::from(card.height)));
        bottom
      })
      .collect();
    Self {
      cards,
      total_height,
      columns,
      bottom_prefix,
    }
  }
  /// Cards are emitted in nondecreasing y order, even in shortest-column masonry.
  /// Prefix maximum bottoms retain tall images that started above the window.
  pub fn visible_indices(&self, scroll: u32, height: u16) -> std::ops::Range<usize> {
    let start = self
      .bottom_prefix
      .partition_point(|bottom| *bottom <= scroll);
    let end = self
      .cards
      .partition_point(|card| card.y < scroll.saturating_add(u32::from(height)));
    start.min(end)..end
  }

  pub fn horizontal_target(&self, focused: usize, right: bool) -> Option<usize> {
    let current = self.cards.get(focused)?;
    let center_y = |card: &CanvasRect| i64::from(card.y) + i64::from(card.height) / 2;
    self
      .cards
      .iter()
      .enumerate()
      .filter(|(_, card)| {
        if right {
          card.x > current.x
        } else {
          card.x < current.x
        }
      })
      .min_by_key(|(_, card)| {
        let gap = if card.y > current.y.saturating_add(u32::from(current.height)) {
          card.y - current.y - u32::from(current.height)
        } else {
          current
            .y
            .saturating_sub(card.y.saturating_add(u32::from(card.height)))
        };
        (
          gap,
          card.x.abs_diff(current.x),
          center_y(card).abs_diff(center_y(current)),
        )
      })
      .map(|(index, _)| index)
  }

  /// Follow the actual spatial navigation rather than assuming adjacent file indices.
  pub fn vertical_target(&self, focused: usize, down: bool) -> Option<usize> {
    let current = self.cards.get(focused)?;
    self
      .cards
      .iter()
      .enumerate()
      .filter(|(index, card)| {
        *index != focused
          && if down {
            card.y > current.y
          } else {
            card.y < current.y
          }
      })
      .min_by_key(|(_, card)| {
        card
          .y
          .abs_diff(current.y)
          .saturating_mul(1000)
          .saturating_add((card.center_x() - current.center_x()).unsigned_abs())
      })
      .map(|(index, _)| index)
  }

  pub fn scroll_for_focus(&self, focused: usize, scroll: u32, height: u16, strategy: &str) -> u32 {
    let Some(card) = self.cards.get(focused) else {
      return scroll;
    };
    if strategy != "masonry" {
      // Fixed rows remain whole: a list is a one-column grid, not a sliced canvas.
      let columns = self.columns.max(1);
      let row = focused / columns;
      let stride = self
        .cards
        .get(columns)
        .map(|card| card.y)
        .unwrap_or(u32::from(card.height))
        .max(1);
      let gap = stride.saturating_sub(u32::from(card.height));
      let visible_rows = ((u32::from(height) + gap) / stride).max(1) as usize;
      let first = (scroll / stride) as usize;
      let first = if row < first {
        row
      } else if row >= first + visible_rows {
        row + 1 - visible_rows
      } else {
        first
      };
      return (first.min(
        self
          .cards
          .len()
          .div_ceil(columns)
          .saturating_sub(visible_rows),
      ) as u32)
        .saturating_mul(stride);
    }
    scroll_for_card(*card, scroll, height)
  }
}

pub fn compute_browser_layout(
  items: &[ImageItem],
  area: Rect,
  config: &EffectiveLayoutConfig,
  cell_pixels: Option<(u16, u16)>,
) -> BrowserLayout {
  match config.strategy.as_str() {
    "masonry" => masonry_layout(items, area.width, config, cell_pixels),
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
  if visible_height == 0 || card.width == 0 || card.x >= viewport.width {
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
  BrowserLayout::new(cards, total_height, columns)
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
  BrowserLayout::new(cards, total_height, 1)
}

fn masonry_layout(
  items: &[ImageItem],
  area_width: u16,
  config: &EffectiveLayoutConfig,
  cell_pixels: Option<(u16, u16)>,
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
    let height = masonry_card_height(item, card_width, config, cell_pixels);
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
  BrowserLayout::new(cards, total_height, columns)
}

fn masonry_card_height(
  item: &ImageItem,
  card_width: u16,
  config: &EffectiveLayoutConfig,
  cell_pixels: Option<(u16, u16)>,
) -> u16 {
  let inset = config.padding.saturating_add(u16::from(config.show_border));
  let content_width = card_width.saturating_sub(inset.saturating_mul(2));
  let has_label = config.show_filename && config.card_style != "image_only";
  let side_label = has_label && matches!(config.filename_position.as_str(), "left" | "right");
  let image_width = if side_label && content_width > 1 {
    ((f32::from(content_width) * config.image_ratio).round() as u16).clamp(1, content_width - 1)
  } else {
    content_width
  };
  let (cw, ch) = cell_pixels.unwrap_or((8, 16));
  let image_height = item
    .dimensions
    .map(|(w, h)| {
      (f64::from(image_width) * f64::from(cw.max(1)) * f64::from(h)
        / f64::from(w.max(1))
        / f64::from(ch.max(1)))
      .ceil() as u16
    })
    .unwrap_or(config.card_height)
    .max(1);
  let content_height = if has_label && !side_label {
    if config.label_lines > 0 {
      image_height.saturating_add(config.label_lines)
    } else {
      (f32::from(image_height) / config.image_ratio).ceil() as u16
    }
  } else {
    image_height
  };
  content_height
    .saturating_add(u16::from(config.show_page_numbers))
    .saturating_add(inset.saturating_mul(2))
    .max(1)
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

/// Scroll needed to expose a focused card; shared with speculative preloading.
pub fn scroll_for_card(card: CanvasRect, scroll: u32, viewport_height: u16) -> u32 {
  let height = u32::from(viewport_height.max(1));
  let top = card.y;
  let bottom = top.saturating_add(u32::from(card.height));
  let visible_bottom = scroll.saturating_add(height);
  if u32::from(card.height) >= height {
    if bottom <= scroll || top >= visible_bottom {
      top
    } else {
      scroll
    }
  } else if top < scroll {
    top
  } else if bottom > visible_bottom {
    bottom.saturating_sub(height)
  } else {
    scroll
  }
}
