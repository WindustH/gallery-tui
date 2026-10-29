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
  layout::{BrowserLayout, screen_rect},
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
  renderer.begin_frame();
  let mut images = FrameImages::new(renderer, tx, app.terminal_cell_pixels);
  let mut cursor_position = None;
  let area = frame.area();
  let footer_height = footer_height(app, area.width).min(area.height);
  // Permanent chrome reserves space; transient footer rows float over it.
  let main = Rect {
    height: area.height.saturating_sub(u16::from(
      app.view != ViewMode::Detail || app.detail_page != DetailPage::Image,
    )),
    ..area
  };
  let footer = Rect::new(
    area.x,
    area.bottom().saturating_sub(footer_height),
    area.width,
    footer_height,
  );
  let completion_area = completion_overlay_area(app, area, footer);

  match app.view {
    ViewMode::Browser => draw_browser(frame, app, &mut images, main),
    ViewMode::Detail => draw_detail(frame, app, &mut images, main),
  }
  draw_footer(frame, app, footer, &mut cursor_position);
  // Modal rects replace kitty U=1 placeholder cells. Uncovered cells keep
  // displaying the image, and the regular text diff restores placeholders
  // when a modal closes.
  let mut occluders = vec![footer];
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
  app.overlay_areas.clone_from(&occluders);
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
  let composition = layout_config.composition.clone();
  let primary_area = composition
    .as_ref()
    .map_or(area, |c| c.split.areas(area)[c.split.primary.index()]);
  let layout_changed = app.prepare_browser_layout(&layout_config, primary_area);
  if let Some(layout) = app.browser_layout() {
    draw_browser_panel(
      frame,
      app,
      images,
      BrowserPanelView {
        layout,
        config: &layout_config,
        area: primary_area,
        scroll: app.browser_scroll,
        following: false,
      },
    );
  }
  if let Some(composition) = composition {
    let secondary = 1 - composition.split.primary.index();
    app.prepare_follower(
      composition.panels[secondary].clone(),
      composition.split.areas(area)[secondary],
    );
    if let Some(panel) = &app.follower {
      draw_browser_panel(
        frame,
        app,
        images,
        BrowserPanelView {
          layout: &panel.layout,
          config: &panel.config,
          area: panel.area,
          scroll: panel.scroll,
          following: true,
        },
      );
    }
  } else {
    app.follower = None;
  }
  // Pending draws request preservation while their replacements render.
  // Apply the layout boundary AFTER both panels draw; doing this beforehand
  // lets Pending re-enable preservation and retain the previous main image
  // inside a larger card of the new layout.
  if layout_changed {
    images.preserve_overlays = false;
    images.preserve_areas.clear();
  }
  // Both panes enqueue their visible images before speculative work starts.
  if let Some(layout) = app.browser_layout() {
    preload_browser_neighbors(
      app,
      images,
      BrowserPanelView {
        layout,
        config: &layout_config,
        area: primary_area,
        scroll: app.browser_scroll,
        following: false,
      },
    );
  }
  if let Some(panel) = &app.follower {
    preload_browser_neighbors(
      app,
      images,
      BrowserPanelView {
        layout: &panel.layout,
        config: &panel.config,
        area: panel.area,
        scroll: panel.scroll,
        following: true,
      },
    );
  }
}

#[derive(Clone, Copy)]
struct BrowserPanelView<'a> {
  layout: &'a BrowserLayout,
  config: &'a EffectiveLayoutConfig,
  area: Rect,
  scroll: u32,
  following: bool,
}

fn draw_browser_panel(
  frame: &mut Frame,
  app: &App,
  images: &mut FrameImages,
  panel: BrowserPanelView<'_>,
) {
  let BrowserPanelView {
    layout,
    config: layout_config,
    area,
    scroll,
    ..
  } = panel;
  if area.is_empty() {
    return;
  }
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

  for index in layout.visible_indices(scroll, area.height) {
    let item = &app.images[index];
    let card_canvas = &layout.cards[index];
    let Some(card_area) = screen_rect(*card_canvas, area, scroll) else {
      continue;
    };
    if layout_config.strategy != "masonry"
      && (card_canvas.y < scroll || card_area.height != card_canvas.height)
    {
      continue;
    }
    let card = CardView {
      index,
      area: card_area,
      full_size: Rect::new(0, 0, card_canvas.width, card_canvas.height),
      hidden_top: scroll
        .saturating_sub(card_canvas.y)
        .min(u32::from(card_canvas.height)) as u16,
    };
    draw_card(frame, app, images, item, card, panel);
  }
}

/// Full card geometry plus its visible screen window.
struct CardView {
  index: usize,
  area: Rect,
  full_size: Rect,
  hidden_top: u16,
}

fn draw_card(
  frame: &mut Frame,
  app: &App,
  images: &mut FrameImages,
  item: &ImageItem,
  card: CardView,
  panel: BrowserPanelView<'_>,
) {
  let layout_config = panel.config;
  let theme = &app.settings.theme;
  let focused = layout_config.highlight && card.index == app.focused;
  let selected = layout_config.highlight && app.selected.contains(&item.path);
  let (foreground, background) = card_state_colors(theme, focused, selected);
  let style = Style::default().fg(foreground).bg(background);
  // Compute content against the full card; clip only its final placement.
  // Do not allocate a full-height cell buffer for very tall masonry images.
  let mut borders = Borders::LEFT | Borders::RIGHT;
  if card.hidden_top == 0 {
    borders |= Borders::TOP;
  }
  if card.hidden_top.saturating_add(card.area.height) == card.full_size.height {
    borders |= Borders::BOTTOM;
  }
  let block = Block::default().style(style);
  frame.render_widget(
    if layout_config.show_border {
      block.borders(borders).border_style(style)
    } else {
      block
    },
    card.area,
  );
  let window = Rect::new(0, card.hidden_top, card.area.width, card.area.height);
  let inner = card_inner_area(card.full_size, layout_config);
  if layout_config.show_page_numbers && inner.y > 0 {
    let number = Rect::new(inner.x, inner.y - 1, inner.width, 1).intersection(window);
    if !number.is_empty() {
      frame.render_widget(
        Paragraph::new(format!(
          "{}{}",
          if focused { "> " } else { "" },
          card.index + 1
        ))
        .style(style),
        Rect::new(
          card.area.x + number.x,
          card.area.y + number.y - card.hidden_top,
          number.width,
          number.height,
        ),
      );
    }
  }
  let (image_area, label_area) = split_card_content(inner, layout_config);
  if let Some(label) = label_area {
    let visible = label.intersection(window);
    if !visible.is_empty() {
      let text = if label.height <= 1 {
        truncate_for_width(&item.file_name, label.width as usize)
      } else {
        wrap_for_area(&item.file_name, label.width as usize, label.height as usize)
      };
      frame.render_widget(
        Paragraph::new(text)
          .style(style)
          .scroll((visible.y - label.y, visible.x - label.x)),
        Rect::new(
          card.area.x + visible.x,
          card.area.y + visible.y - card.hidden_top,
          visible.width,
          visible.height,
        ),
      );
    }
  }
  if layout_config.strategy != "masonry" {
    images.draw(
      frame,
      item,
      Rect::new(
        card.area.x + image_area.x,
        card.area.y + image_area.y,
        image_area.width,
        image_area.height,
      ),
      image_alignment_for_layout(layout_config),
      (card.index, app.images.len()),
    );
    return;
  }
  let fitted = image::fit_image_rect(
    image_area,
    item,
    app.terminal_cell_pixels,
    image_alignment_for_layout(layout_config),
  );
  let visible = fitted.intersection(Rect::new(
    0,
    card.hidden_top,
    card.area.width,
    card.area.height,
  ));
  if !visible.is_empty() {
    let viewport = img_tui::NativeImageViewport {
      full_width_cells: fitted.width,
      full_height_cells: fitted.height,
      visible_width_cells: visible.width,
      visible_height_cells: visible.height,
      left_cells: u32::from(visible.x - fitted.x),
      top_cells: u32::from(visible.y - fitted.y),
    };
    let limit = (panel.area.height / layout_config.scroll_divisor).max(1);
    for slice in viewport.slices(limit) {
      images.draw_window(
        frame,
        item,
        slice
          .screen_area(
            viewport,
            Rect::new(
              card.area.x + visible.x,
              card.area.y + visible.y - card.hidden_top,
              visible.width,
              visible.height,
            ),
          )
          .expect("slice is inside its visible window"),
        slice,
        (card.index, app.images.len()),
      );
    }
  }
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

fn draw_detail(frame: &mut Frame, app: &mut App, images: &mut FrameImages, area: Rect) {
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
        .constraints([
          Constraint::Length(if area.width < 62 { 0 } else { 42 }),
          Constraint::Min(20),
        ])
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
      let (scroll, max_scroll) = draw_metadata(
        frame,
        &app.settings.theme,
        item,
        split[1],
        app.metadata_scroll,
      );
      app.metadata_scroll = scroll;
      app.metadata_max_scroll = max_scroll;
      app.metadata_view_height = split[1].height.saturating_sub(2).max(1);
    }
  }
}

fn preload_browser_neighbors(app: &App, images: &mut FrameImages, panel: BrowserPanelView<'_>) {
  let BrowserPanelView {
    layout,
    config: layout_config,
    area,
    scroll,
    ..
  } = panel;
  if area.is_empty() {
    return;
  }
  if !images.can_preload() {
    return;
  }
  let render = &app.settings.config.render;
  let alignment = image_alignment_for_layout(layout_config);
  if layout_config.strategy != "masonry" {
    let visible = layout.visible_indices(scroll, area.height);
    if visible.is_empty() {
      return;
    }
    let first = visible.start;
    let last = visible.end - 1;
    let start = first.saturating_sub(render.preload_behind);
    let end = last
      .saturating_add(render.preload_ahead)
      .min(app.images.len().saturating_sub(1));
    for index in start..=end {
      let (Some(card), Some(item)) = (layout.cards.get(index), app.images.get(index)) else {
        continue;
      };
      let inner = card_inner_area(Rect::new(0, 0, card.width, card.height), layout_config);
      let (image_area, _) = split_card_content(inner, layout_config);
      images.preload(item, image_area, alignment);
    }
    return;
  }
  // Predict the real spatial up/down path, with each step starting at the
  // preceding step's scroll position. Masonry file indices are not row order.
  let candidates = if panel.following {
    app.follower_preload_scrolls()
  } else {
    let mut down = Some((app.focused, scroll));
    let mut up = down;
    let mut candidates = Vec::new();
    for distance in 1..=render
      .preload_ahead
      .max(render.preload_behind)
      .min(app.images.len())
    {
      for (route, forward, count) in [
        (&mut down, true, render.preload_ahead),
        (&mut up, false, render.preload_behind),
      ] {
        if distance > count {
          continue;
        }
        if let Some((focused, previous_scroll)) = *route {
          *route = layout.vertical_target(focused, forward).map(|next| {
            let next_scroll =
              layout.scroll_for_focus(next, previous_scroll, area.height, &layout_config.strategy);
            candidates.push(next_scroll);
            (next, next_scroll)
          });
        }
      }
    }
    candidates
  };
  let mut scrolls = std::collections::HashSet::new();
  scrolls.insert(scroll);
  for scroll in candidates {
    if !scrolls.insert(scroll) {
      continue;
    }
    for index in layout.visible_indices(scroll, area.height) {
      let item = &app.images[index];
      let card = &layout.cards[index];
      if !images.can_preload() {
        return;
      }
      let Some(screen) = screen_rect(*card, area, scroll) else {
        continue;
      };
      let hidden_top = scroll.saturating_sub(card.y) as u16;
      let inner = card_inner_area(Rect::new(0, 0, card.width, card.height), layout_config);
      let (image_area, _) = split_card_content(inner, layout_config);
      let fitted = image::fit_image_rect(image_area, item, app.terminal_cell_pixels, alignment);
      let visible = fitted.intersection(Rect::new(0, hidden_top, screen.width, screen.height));
      if visible.is_empty() {
        continue;
      }
      images.preload_window(
        item,
        img_tui::NativeImageViewport {
          full_width_cells: fitted.width,
          full_height_cells: fitted.height,
          visible_width_cells: visible.width,
          visible_height_cells: visible.height,
          left_cells: u32::from(visible.x - fitted.x),
          top_cells: u32::from(visible.y - fitted.y),
        },
        (area.height / layout_config.scroll_divisor).max(1),
      );
    }
  }
}

fn preload_detail_neighbors(app: &App, images: &mut FrameImages, image_area: Rect) {
  if image_area.width == 0 || image_area.height == 0 || app.images.is_empty() {
    return;
  }
  if !images.can_preload() {
    return;
  }
  let render = &app.settings.config.render;
  let start = app.focused.saturating_sub(render.preload_behind);
  let end = app
    .focused
    .saturating_add(render.preload_ahead)
    .min(app.images.len() - 1);
  for index in (start..=end).filter(|index| *index != app.focused) {
    if let Some(item) = app.images.get(index) {
      images.preload(item, image_area, ImageAlignment::Center);
    }
  }
}

fn draw_metadata(
  frame: &mut Frame,
  theme: &ThemeConfig,
  item: &ImageItem,
  area: Rect,
  scroll: u16,
) -> (u16, u16) {
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
  let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
  let max_scroll = paragraph
    .line_count(area.width.saturating_sub(2))
    .saturating_sub(usize::from(area.height.saturating_sub(2)))
    .min(usize::from(u16::MAX)) as u16;
  let scroll = scroll.min(max_scroll);
  frame.render_widget(
    paragraph.scroll((scroll, 0)).block(
      Block::default()
        .borders(Borders::ALL)
        .title("metadata")
        .border_style(Style::default().fg(theme.color(&theme.border))),
    ),
    area,
  );
  (scroll, max_scroll)
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
  use framework_tui::text::{display_width, truncate_to_width};
  if width == 0 || height == 0 {
    return String::new();
  }
  let mut lines = Vec::new();
  let mut line = String::new();
  let mut used = 0;
  for (index, ch) in value.char_indices() {
    let columns = display_width(ch.encode_utf8(&mut [0; 4]));
    if ch == '\n' || used + columns > width {
      if lines.len() + 1 == height {
        line.push_str(&value[index..].replace('\n', " "));
        lines.push(truncate_to_width(&line, width).into_owned());
        return lines.join("\n");
      }
      lines.push(std::mem::take(&mut line));
      used = 0;
      if ch == '\n' {
        continue;
      }
    }
    if columns <= width {
      line.push(ch);
      used += columns;
    }
  }
  if !line.is_empty() {
    lines.push(line);
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
  framework_tui::text::truncate_to_width(value, width).into_owned()
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
  let inner = safe_inner(area, layout.padding, layout.padding);
  let label = u16::from(layout.show_page_numbers).min(inner.height);
  Rect::new(
    inner.x,
    inner.y.saturating_add(label),
    inner.width,
    inner.height.saturating_sub(label),
  )
}

/// Continue speculative work after background completions without drawing a frame.
pub fn pump_preload(app: &App, renderer: &mut RenderStore, tx: &mpsc::UnboundedSender<AsyncEvent>) {
  if app.view != ViewMode::Browser {
    return;
  }
  let (Some(layout), Some(area)) = (app.browser_layout(), app.browser_viewport()) else {
    return;
  };
  let config = app.settings.config.layout.effective();
  let mut images = FrameImages::new(renderer, tx, app.terminal_cell_pixels);
  preload_browser_neighbors(
    app,
    &mut images,
    BrowserPanelView {
      layout,
      config: &config,
      area,
      scroll: app.browser_scroll,
      following: false,
    },
  );
  if let Some(panel) = &app.follower {
    preload_browser_neighbors(
      app,
      &mut images,
      BrowserPanelView {
        layout: &panel.layout,
        config: &panel.config,
        area: panel.area,
        scroll: panel.scroll,
        following: true,
      },
    );
  }
}
