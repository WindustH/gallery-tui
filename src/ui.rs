use std::time::SystemTime;

use chrono::{DateTime, Local};
use humansize::{DECIMAL, format_size};
use ratatui::{
  Frame,
  layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
  style::{Color, Style},
  text::{Line, Span, Text},
  widgets::{Block, Borders, Paragraph, Wrap},
};
use tokio::sync::mpsc;

use crate::{
  app::{App, DetailPage, ViewMode},
  config::{EffectiveLayoutConfig, ThemeConfig},
  event::AsyncEvent,
  layout::{BrowserLayout, compute_browser_layout, screen_rect},
  model::ImageItem,
  render::RenderStore,
  terminal::FrameOutput,
};

mod footer;
mod image;
mod modal;

use footer::{completion_overlay_area, draw_command_completion, draw_footer, footer_height};
use image::{FrameImages, ImageAlignment, image_alignment_for_layout};
use modal::{draw_confirm, draw_key_help};

pub fn draw(
  frame: &mut Frame,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
) -> FrameOutput {
  let mut images = FrameImages::new(renderer, tx, app.terminal_cell_pixels);
  let mut cursor_position = None;
  let area = frame.area();
  let footer_height = footer_height(app, area.width).min(area.height);
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(1), Constraint::Length(footer_height)])
    .split(area);
  let main = chunks[0];
  let footer = chunks[1];
  let completion_area = completion_overlay_area(app, area, footer);

  match app.view {
    ViewMode::Browser => draw_browser(frame, app, &mut images, main),
    ViewMode::Detail => draw_detail(frame, app, &mut images, main),
  }
  draw_footer(frame, app, footer, &mut cursor_position);
  // Modal rects replace kitty U=1 placeholder cells. Uncovered cells keep
  // displaying the image, and the regular text diff restores placeholders
  // when a modal closes.
  let mut occluders = Vec::new();
  if let Some(completion_area) = completion_area {
    draw_command_completion(frame, app, completion_area);
    occluders.push(completion_area);
  }
  if let Some(confirm_area) = draw_confirm(frame, app, area) {
    occluders.push(confirm_area);
  }
  if let Some(help_area) = draw_key_help(frame, app, area) {
    occluders.push(help_area);
  }
  if app.confirm.is_some() || app.key_help {
    // Modals own the interaction: hide the cursor while they are open.
    cursor_position = None;
  }
  let mut output = FrameOutput::new(images.overlays, cursor_position);
  output.preserve_overlays = images.preserve_overlays;
  if images.preserve_overlays {
    output.preserve_areas = images.preserve_areas;
  }
  output.occluders = occluders;
  output
}

fn fill_background(frame: &mut Frame, theme: &ThemeConfig, area: Rect) {
  frame.render_widget(
    Block::default().style(
      Style::default()
        .bg(theme.color(&theme.background))
        .fg(theme.color(&theme.foreground)),
    ),
    area,
  );
}

fn draw_browser(frame: &mut Frame, app: &mut App, images: &mut FrameImages, area: Rect) {
  fill_background(frame, &app.settings.theme, area);

  let layout_config = app.settings.config.layout.effective();
  let layout = compute_browser_layout(&app.images, area, &layout_config);
  let layout_changed = app.update_browser_layout(layout, area);
  let app = &*app;
  let Some(layout) = app.browser_layout() else {
    return;
  };

  if app.images.is_empty() {
    let muted = app.settings.theme.color(&app.settings.theme.muted);
    frame.render_widget(
      Paragraph::new("No images found")
        .alignment(Alignment::Center)
        .style(Style::default().fg(muted)),
      area,
    );
    return;
  }

  for (index, (item, card_canvas)) in app.images.iter().zip(&layout.cards).enumerate() {
    let Some(card_area) = screen_rect(*card_canvas, area, app.browser_scroll) else {
      continue;
    };
    let card = CardView {
      index,
      area: card_area,
      top_clipped: card_canvas.y < app.browser_scroll,
      bottom_clipped: card_canvas.y.saturating_add(u32::from(card_canvas.height))
        > app.browser_scroll.saturating_add(u32::from(area.height)),
    };
    draw_card(frame, app, images, item, card, &layout_config);
  }
  if layout_changed {
    images.preserve_overlays = false;
    images.preserve_areas.clear();
  }
  preload_browser_neighbors(app, images, layout, &layout_config);
}

/// A card's on-screen rect and whether the viewport cuts off its edges.
struct CardView {
  index: usize,
  area: Rect,
  top_clipped: bool,
  bottom_clipped: bool,
}

fn draw_card(
  frame: &mut Frame,
  app: &App,
  images: &mut FrameImages,
  item: &ImageItem,
  card: CardView,
  layout_config: &EffectiveLayoutConfig,
) {
  let theme = &app.settings.theme;
  let focused = card.index == app.focused;
  let selected = app.selected.contains(&item.path);
  let (foreground, background) = card_state_colors(theme, focused, selected);
  let style = Style::default().fg(foreground).bg(background);
  if layout_config.show_border {
    let block = Block::default()
      .borders(Borders::ALL)
      .border_style(style)
      .style(style);
    frame.render_widget(block, card.area);
    clear_clipped_card_edges(frame, theme, &card);
  } else {
    frame.render_widget(Block::default().style(style), card.area);
  }

  let inner = card_inner_area(card.area, layout_config);
  if inner.width == 0 || inner.height == 0 {
    return;
  }
  let (image_area, label_area) = split_card_content(inner, layout_config);
  if let Some(label_area) = label_area {
    draw_label(frame, &item.file_name, label_area, style);
  }
  images.draw(
    frame,
    item,
    image_area,
    image_alignment_for_layout(layout_config),
    (card.index, app.images.len()),
  );
}

fn card_state_colors(theme: &ThemeConfig, focused: bool, selected: bool) -> (Color, Color) {
  if focused && selected {
    return state_colors(
      theme,
      &theme.hover_selected_foreground,
      &theme.hover_selected_background,
    );
  }
  if focused {
    return state_colors(theme, &theme.hover_foreground, &theme.hover_background);
  }
  if selected {
    return state_colors(
      theme,
      &theme.selected_foreground,
      &theme.selected_background,
    );
  }
  (
    theme.color(&theme.foreground),
    theme.color(&theme.background),
  )
}

fn state_colors(theme: &ThemeConfig, foreground: &str, background: &str) -> (Color, Color) {
  (
    theme.foreground_color(foreground, background),
    theme.color(background),
  )
}

fn draw_detail(frame: &mut Frame, app: &App, images: &mut FrameImages, area: Rect) {
  fill_background(frame, &app.settings.theme, area);

  let Some(item) = app.current() else {
    frame.render_widget(Paragraph::new("No image selected"), area);
    return;
  };
  let position = (app.focused, app.images.len());

  match app.detail_page {
    DetailPage::Image => {
      images.draw(frame, item, area, ImageAlignment::Center, position);
      preload_detail_neighbors(app, images, area);
    }
    DetailPage::Metadata => {
      let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(area.width.min(42)), Constraint::Min(20)])
        .split(area);
      let preview = safe_inner(split[0], 1, 1);
      frame.render_widget(
        Block::default()
          .borders(Borders::ALL)
          .title("preview")
          .border_style(Style::default().fg(app.settings.theme.color(&app.settings.theme.border))),
        split[0],
      );
      images.draw(frame, item, preview, ImageAlignment::Center, position);
      preload_detail_neighbors(app, images, preview);
      draw_metadata(frame, &app.settings.theme, item, split[1]);
    }
  }
}

fn preload_browser_neighbors(
  app: &App,
  images: &mut FrameImages,
  layout: &BrowserLayout,
  layout_config: &EffectiveLayoutConfig,
) {
  let render = &app.settings.config.render;
  let start = app.focused.saturating_sub(render.preload_behind);
  let end = app
    .focused
    .saturating_add(render.preload_ahead)
    .min(app.images.len().saturating_sub(1));
  let alignment = image_alignment_for_layout(layout_config);
  for index in start..=end {
    let (Some(card), Some(item)) = (layout.cards.get(index), app.images.get(index)) else {
      continue;
    };
    let inner = card_inner_area(Rect::new(0, 0, card.width, card.height), layout_config);
    let (image_area, _) = split_card_content(inner, layout_config);
    images.preload(item, image_area, alignment);
  }
}

fn preload_detail_neighbors(app: &App, images: &mut FrameImages, image_area: Rect) {
  if image_area.width == 0 || image_area.height == 0 || app.images.is_empty() {
    return;
  }
  let render = &app.settings.config.render;
  let start = app.focused.saturating_sub(render.preload_behind.min(1));
  let end = app
    .focused
    .saturating_add(render.preload_ahead.min(2))
    .min(app.images.len() - 1);
  for index in (start..=end).filter(|index| *index != app.focused) {
    if let Some(item) = app.images.get(index) {
      images.preload(item, image_area, ImageAlignment::Center);
    }
  }
}

fn draw_metadata(frame: &mut Frame, theme: &ThemeConfig, item: &ImageItem, area: Rect) {
  let mut lines = vec![
    metadata_line("file", &item.file_name, theme),
    metadata_line("path", &item.path.display().to_string(), theme),
    metadata_line("format", &item.extension, theme),
    metadata_line("size", &format_size(item.size_bytes, DECIMAL), theme),
    metadata_line(
      "dimensions",
      &item
        .dimensions
        .map(|(w, h)| format!("{w} x {h}"))
        .unwrap_or_else(|| "unknown".to_string()),
      theme,
    ),
    metadata_line("modified", &format_time(item.modified), theme),
    metadata_line("created", &format_time(item.created), theme),
  ];
  if item.metadata.is_empty() {
    lines.push(metadata_line("metadata", "none", theme));
  } else {
    lines.push(metadata_line(
      "metadata",
      &format!("{} EXIF tags", item.metadata.len()),
      theme,
    ));
    for entry in &item.metadata {
      lines.push(metadata_line(
        &format!("{}.{}", entry.group, entry.name),
        &entry.value,
        theme,
      ));
    }
  }
  frame.render_widget(
    Paragraph::new(Text::from(lines))
      .block(
        Block::default()
          .borders(Borders::ALL)
          .title("metadata")
          .border_style(Style::default().fg(theme.color(&theme.border))),
      )
      .wrap(Wrap { trim: false }),
    area,
  );
}

fn clear_clipped_card_edges(frame: &mut Frame, theme: &ThemeConfig, card: &CardView) {
  let style = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.background));
  let area = card.area;
  if card.top_clipped {
    clear_row(frame, area, area.y, style);
  }
  if card.bottom_clipped && area.height > 1 {
    clear_row(
      frame,
      area,
      area.y.saturating_add(area.height.saturating_sub(1)),
      style,
    );
  }
}

fn clear_row(frame: &mut Frame, area: Rect, y: u16, style: Style) {
  let buf = frame.buffer_mut();
  for x in area.x..area.x.saturating_add(area.width) {
    if let Some(cell) = buf.cell_mut((x, y)) {
      cell.set_symbol(" ");
      cell.set_style(style);
    }
  }
}

fn draw_label(frame: &mut Frame, label: &str, area: Rect, style: Style) {
  if area.width == 0 || area.height == 0 {
    return;
  }
  frame.render_widget(Block::default().style(style), area);
  let text = if area.height <= 1 {
    truncate_for_width(label, area.width as usize)
  } else {
    wrap_for_area(label, area.width as usize, area.height as usize)
  };
  frame.render_widget(
    Paragraph::new(text).style(style).wrap(Wrap { trim: false }),
    area,
  );
}

fn split_card_content(area: Rect, layout: &EffectiveLayoutConfig) -> (Rect, Option<Rect>) {
  if !layout.show_filename || layout.card_style == "image_only" {
    return (area, None);
  }
  match layout.filename_position.as_str() {
    "top" => {
      let (image_height, label_height) =
        split_vertical_image_label_len(area.height, layout.image_ratio, layout.label_lines);
      if label_height == 0 {
        return (area, None);
      }
      let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
          Constraint::Length(label_height),
          Constraint::Length(image_height),
        ])
        .split(area);
      (chunks[1], Some(chunks[0]))
    }
    "left" => {
      let (image_width, label_width) = split_image_label_len(area.width, layout.image_ratio);
      if label_width == 0 {
        return (area, None);
      }
      let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
          Constraint::Length(label_width),
          Constraint::Length(image_width),
        ])
        .split(area);
      (chunks[1], Some(chunks[0]))
    }
    "right" => {
      let (image_width, label_width) = split_image_label_len(area.width, layout.image_ratio);
      if label_width == 0 {
        return (area, None);
      }
      let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
          Constraint::Length(image_width),
          Constraint::Length(label_width),
        ])
        .split(area);
      (chunks[0], Some(chunks[1]))
    }
    _ => {
      let (image_height, label_height) =
        split_vertical_image_label_len(area.height, layout.image_ratio, layout.label_lines);
      if label_height == 0 {
        return (area, None);
      }
      let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
          Constraint::Length(image_height),
          Constraint::Length(label_height),
        ])
        .split(area);
      (chunks[0], Some(chunks[1]))
    }
  }
}

fn split_vertical_image_label_len(total: u16, image_ratio: f32, label_lines: u16) -> (u16, u16) {
  if total <= 1 {
    return (total, 0);
  }
  if label_lines > 0 {
    let label = label_lines.min(total - 1);
    return (total.saturating_sub(label), label);
  }
  split_image_label_len(total, image_ratio)
}

fn split_image_label_len(total: u16, image_ratio: f32) -> (u16, u16) {
  if total <= 1 {
    return (total, 0);
  }
  let image_ratio = image_ratio.clamp(0.1, 0.95);
  let image = ((f32::from(total) * image_ratio).round() as u16).clamp(1, total - 1);
  (image, total.saturating_sub(image))
}

fn wrap_for_area(value: &str, width: usize, height: usize) -> String {
  if width == 0 || height == 0 {
    return String::new();
  }
  let mut lines = Vec::new();
  let mut line = String::new();
  for ch in value.chars() {
    if line.chars().count() >= width {
      lines.push(line);
      line = String::new();
      if lines.len() >= height {
        break;
      }
    }
    line.push(ch);
  }
  if lines.len() < height && !line.is_empty() {
    lines.push(line);
  }
  if lines.len() > height {
    lines.truncate(height);
  }
  if let Some(last) = lines.last_mut()
    && value.chars().count() > width.saturating_mul(height)
  {
    *last = truncate_for_width(last, width);
  }
  lines.join("\n")
}

fn metadata_line(label: &str, value: &str, theme: &ThemeConfig) -> Line<'static> {
  Line::from(vec![
    Span::styled(
      format!("{label:>10}  "),
      Style::default().fg(theme.color(&theme.muted)),
    ),
    Span::raw(value.to_string()),
  ])
}

fn format_time(value: Option<SystemTime>) -> String {
  value
    .map(|time| {
      let datetime: DateTime<Local> = time.into();
      datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    })
    .unwrap_or_else(|| "unknown".to_string())
}

fn truncate_for_width(value: &str, width: usize) -> String {
  if width == 0 {
    return String::new();
  }
  if width <= 3 {
    return value.chars().take(width).collect();
  }
  let mut out = String::new();
  for ch in value.chars() {
    if out.chars().count() + 1 >= width {
      out.push_str("...");
      return out;
    }
    out.push(ch);
  }
  out
}

fn safe_inner(area: Rect, horizontal: u16, vertical: u16) -> Rect {
  if area.width <= horizontal.saturating_mul(2) || area.height <= vertical.saturating_mul(2) {
    return Rect::new(area.x, area.y, 0, 0);
  }
  area.inner(Margin {
    horizontal,
    vertical,
  })
}

fn card_inner_area(area: Rect, layout: &EffectiveLayoutConfig) -> Rect {
  let area = if layout.show_border {
    safe_inner(area, 1, 1)
  } else {
    area
  };
  safe_inner(area, layout.padding, layout.padding)
}
