use std::time::SystemTime;

use chrono::{DateTime, Local};
use humansize::{DECIMAL, format_size};
use ratatui::{
  Frame,
  buffer::CellDiffOption,
  layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tokio::sync::mpsc;

use crate::{
  app::ConfirmDialog,
  app::{App, DetailPage, Prompt, ViewMode},
  config::{EffectiveLayoutConfig, ThemeConfig},
  event::{AsyncEvent, ProtocolOverlay, RenderedImage},
  keymap::KeyHint,
  layout::{compute_browser_layout, screen_rect},
  model::ImageItem,
  render::RenderStore,
};

pub fn draw(
  frame: &mut Frame,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
) {
  let mut protocol_overlays = Vec::new();
  let area = frame.area();
  let footer_height = footer_height(app, area.width).min(area.height);
  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(1), Constraint::Length(footer_height)])
    .split(area);
  let main = chunks[0];
  let footer = chunks[1];

  match app.view {
    ViewMode::Browser => draw_browser(frame, app, renderer, tx, main, &mut protocol_overlays),
    ViewMode::Detail => draw_detail(frame, app, renderer, tx, main, &mut protocol_overlays),
  }
  draw_footer(frame, app, footer);
  draw_confirm(frame, app, area);
  if app.confirm.is_some() {
    protocol_overlays.clear();
  }
  app.protocol_overlays = protocol_overlays;
}

fn footer_height(app: &App, width: u16) -> u16 {
  let status = u16::from(status_visible(app));
  let prompt = u16::from(app.prompt.is_some());
  let completion = command_completion_rows(app);
  let which = if app.hints.is_empty() {
    0
  } else {
    which_key_rows_for_count(app.hints.len(), which_key_columns(app, width))
  };
  status
    .saturating_add(prompt)
    .saturating_add(completion)
    .saturating_add(which)
}

fn command_completion_rows(app: &App) -> u16 {
  app
    .command_completion
    .as_ref()
    .filter(|completion| app.prompt.is_some() && !completion.candidates.is_empty())
    .map(|completion| completion.candidates.len().min(5) as u16)
    .unwrap_or(0)
}

fn status_visible(app: &App) -> bool {
  !(app.view == ViewMode::Detail
    && app.detail_page == DetailPage::Image
    && app.prompt.is_none()
    && app.hints.is_empty())
}

fn which_key_columns(app: &App, width: u16) -> usize {
  let configured = app.settings.theme.which_key_columns.max(1) as usize;
  let max_by_width = (usize::from(width) / 24).max(1);
  configured.min(max_by_width).max(1)
}

fn which_key_rows_for_count(count: usize, columns: usize) -> u16 {
  count.div_ceil(columns.max(1)).max(1) as u16
}

fn draw_browser(
  frame: &mut Frame,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  area: Rect,
  protocol_overlays: &mut Vec<ProtocolOverlay>,
) {
  let bg = app.settings.theme.color(&app.settings.theme.background);
  let foreground = app.settings.theme.color(&app.settings.theme.foreground);
  frame.render_widget(
    Block::default().style(Style::default().bg(bg).fg(foreground)),
    area,
  );

  let layout_config = app.settings.config.layout.effective();
  let layout = compute_browser_layout(&app.images, area, &layout_config);
  app.update_browser_layout(layout.clone(), area);

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

  for (index, item) in app.images.iter().enumerate() {
    let Some(card_canvas) = layout.cards.get(index).copied() else {
      continue;
    };
    let Some(card_area) = screen_rect(card_canvas, area, app.browser_scroll) else {
      continue;
    };
    let top_visible = card_canvas.y >= app.browser_scroll;
    let bottom_visible = card_canvas.y.saturating_add(u32::from(card_canvas.height))
      <= app.browser_scroll.saturating_add(u32::from(area.height));
    draw_card(
      frame,
      app,
      renderer,
      tx,
      index,
      item,
      card_area,
      top_visible,
      bottom_visible,
      &layout_config,
      protocol_overlays,
    );
  }
  preload_browser_neighbors(app, renderer, tx, &layout_config);
}

fn draw_card(
  frame: &mut Frame,
  app: &App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  index: usize,
  item: &ImageItem,
  area: Rect,
  top_visible: bool,
  bottom_visible: bool,
  layout_config: &EffectiveLayoutConfig,
  protocol_overlays: &mut Vec<ProtocolOverlay>,
) {
  let theme = &app.settings.theme;
  let focused = index == app.focused;
  let selected = app.selected.contains(&item.path);
  let (foreground, background) = card_state_colors(theme, focused, selected);
  let style = Style::default().fg(foreground).bg(background);
  let border_style = Style::default().fg(foreground).bg(background);
  if layout_config.show_border {
    let block = Block::default()
      .borders(Borders::ALL)
      .border_style(border_style)
      .style(style);
    frame.render_widget(block, area);
    clear_clipped_card_edges(frame, app, area, top_visible, bottom_visible);
  } else {
    frame.render_widget(Block::default().style(style), area);
  }

  let inner = card_inner_area(area, layout_config);
  if inner.width == 0 || inner.height == 0 {
    return;
  }
  let (image_area, label_area) = split_card_content(inner, layout_config);
  if let Some(label_area) = label_area {
    draw_label(frame, &item.file_name, label_area, style);
  }
  draw_rendered_image(
    frame,
    app,
    renderer,
    tx,
    item,
    image_area,
    0,
    image_alignment_for_layout(layout_config),
    protocol_overlays,
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

fn draw_detail(
  frame: &mut Frame,
  app: &mut App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  area: Rect,
  protocol_overlays: &mut Vec<ProtocolOverlay>,
) {
  let Some(item) = app.current() else {
    frame.render_widget(Paragraph::new("No image selected"), area);
    return;
  };

  match app.detail_page {
    DetailPage::Image => {
      draw_rendered_image(
        frame,
        app,
        renderer,
        tx,
        item,
        area,
        0,
        ImageAlignment::Center,
        protocol_overlays,
      );
      preload_detail_neighbors(app, renderer, tx, area);
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
      draw_rendered_image(
        frame,
        app,
        renderer,
        tx,
        item,
        preview,
        0,
        ImageAlignment::Center,
        protocol_overlays,
      );
      preload_detail_neighbors(app, renderer, tx, preview);
      draw_metadata(frame, app, item, split[1]);
    }
  }
}

fn preload_browser_neighbors(
  app: &App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  layout_config: &EffectiveLayoutConfig,
) {
  let Some(layout) = &app.last_layout else {
    return;
  };
  if app.images.is_empty() {
    return;
  }

  let start = app
    .focused
    .saturating_sub(app.settings.config.render.preload_behind);
  let end = app
    .focused
    .saturating_add(app.settings.config.render.preload_ahead)
    .min(app.images.len().saturating_sub(1));

  for index in start..=end {
    let Some(card) = layout.cards.get(index).copied() else {
      continue;
    };
    let card_area = Rect::new(0, 0, card.width, card.height);
    let inner = card_inner_area(card_area, layout_config);
    let (image_area, _) = split_card_content(inner, layout_config);
    if let Some(item) = app.images.get(index) {
      let fitted = fit_image_rect(
        image_area,
        item,
        app,
        image_alignment_for_layout(layout_config),
      );
      renderer.preload(item, fitted.width, fitted.height, tx);
    }
  }
}

fn preload_detail_neighbors(
  app: &App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  image_area: Rect,
) {
  if image_area.width == 0 || image_area.height == 0 || app.images.is_empty() {
    return;
  }

  let start = app
    .focused
    .saturating_sub(app.settings.config.render.preload_behind.min(1));
  let end = app
    .focused
    .saturating_add(app.settings.config.render.preload_ahead.min(2))
    .min(app.images.len().saturating_sub(1));

  for index in start..=end {
    if index == app.focused {
      continue;
    }
    if let Some(item) = app.images.get(index) {
      let fitted = fit_image_rect(image_area, item, app, ImageAlignment::Center);
      renderer.preload(item, fitted.width, fitted.height, tx);
    }
  }
}

fn draw_metadata(frame: &mut Frame, app: &App, item: &ImageItem, area: Rect) {
  let theme = &app.settings.theme;
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

fn draw_rendered_image(
  frame: &mut Frame,
  app: &App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  item: &ImageItem,
  area: Rect,
  scroll: u16,
  alignment: ImageAlignment,
  protocol_overlays: &mut Vec<ProtocolOverlay>,
) {
  if area.width == 0 || area.height == 0 {
    return;
  }

  let image_area = fit_image_rect(area, item, app, alignment);
  if image_area.width == 0 || image_area.height == 0 {
    return;
  }

  renderer.request(item, image_area.width, image_area.height, tx);
  if let Some(rendered) = renderer.get(item, image_area.width, image_area.height) {
    match rendered {
      RenderedImage::Symbols { mode, text } => {
        let _mode_label = mode.label();
        frame.render_widget(Paragraph::new(text.clone()).scroll((scroll, 0)), image_area);
      }
      RenderedImage::Protocol {
        mode,
        data,
        fingerprint,
        erase,
      } => {
        let _mode_label = mode.label();
        reserve_protocol_area(frame, image_area);
        protocol_overlays.push(ProtocolOverlay {
          area: image_area,
          mode: *mode,
          data: data.clone(),
          fingerprint: *fingerprint,
          erase: erase.clone(),
        });
      }
    }
  } else if let Some(error) = renderer.failure(item, image_area.width, image_area.height) {
    frame.render_widget(
      Paragraph::new(format!("render failed\n{error}")).wrap(Wrap { trim: true }),
      image_area,
    );
  } else {
    frame.render_widget(
      Paragraph::new("rendering...")
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM)),
      image_area,
    );
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageAlignment {
  Left,
  Center,
}

fn image_alignment_for_layout(layout: &EffectiveLayoutConfig) -> ImageAlignment {
  match layout.image_alignment.as_str() {
    "left" => ImageAlignment::Left,
    _ => ImageAlignment::Center,
  }
}

fn fit_image_rect(area: Rect, item: &ImageItem, app: &App, alignment: ImageAlignment) -> Rect {
  let Some((image_width, image_height)) = item.dimensions else {
    return area;
  };
  if image_width == 0 || image_height == 0 || area.width == 0 || area.height == 0 {
    return area;
  }

  let (cell_width, cell_height) = app.terminal_cell_pixels.unwrap_or((8, 16));
  let max_pixel_width = f64::from(area.width) * f64::from(cell_width.max(1));
  let max_pixel_height = f64::from(area.height) * f64::from(cell_height.max(1));
  let scale = (max_pixel_width / f64::from(image_width))
    .min(max_pixel_height / f64::from(image_height))
    .max(0.0);

  let fitted_width = ((f64::from(image_width) * scale) / f64::from(cell_width.max(1)))
    .round()
    .clamp(1.0, f64::from(area.width)) as u16;
  let fitted_height = ((f64::from(image_height) * scale) / f64::from(cell_height.max(1)))
    .round()
    .clamp(1.0, f64::from(area.height)) as u16;

  Rect {
    x: match alignment {
      ImageAlignment::Left => area.x,
      ImageAlignment::Center => area.x + area.width.saturating_sub(fitted_width) / 2,
    },
    y: area.y + area.height.saturating_sub(fitted_height) / 2,
    width: fitted_width,
    height: fitted_height,
  }
}

fn reserve_protocol_area(frame: &mut Frame, area: Rect) {
  let buf = frame.buffer_mut();
  for y in area.y..area.y.saturating_add(area.height) {
    for x in area.x..area.x.saturating_add(area.width) {
      if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_diff_option(CellDiffOption::Skip);
      }
    }
  }
}

fn clear_clipped_card_edges(
  frame: &mut Frame,
  app: &App,
  area: Rect,
  top_visible: bool,
  bottom_visible: bool,
) {
  let theme = &app.settings.theme;
  let style = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.background));
  if !top_visible {
    clear_row(frame, area, area.y, style);
  }
  if !bottom_visible && area.height > 1 {
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

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
  if area.height == 0 {
    return;
  }
  let theme = &app.settings.theme;
  let status_height = u16::from(status_visible(app));
  let status_area = if status_height == 1 {
    Some(Rect::new(
      area.x,
      area.y + area.height.saturating_sub(1),
      area.width,
      1,
    ))
  } else {
    None
  };
  let mut content_bottom = area
    .y
    .saturating_add(area.height.saturating_sub(status_height));

  if let Some(prompt) = &app.prompt
    && content_bottom > area.y
  {
    content_bottom = content_bottom.saturating_sub(1);
    let prompt_area = Rect::new(area.x, content_bottom, area.width, 1);
    draw_prompt(frame, app, prompt, prompt_area);
  }

  let completion_rows = command_completion_rows(app);
  if completion_rows > 0 && content_bottom > area.y {
    let height = completion_rows.min(content_bottom - area.y);
    content_bottom = content_bottom.saturating_sub(height);
    let completion_area = Rect::new(area.x, content_bottom, area.width, height);
    draw_command_completion(frame, app, completion_area);
  }

  if !app.hints.is_empty() && area.y < content_bottom {
    let which_area = Rect::new(area.x, area.y, area.width, content_bottom - area.y);
    draw_which_key(frame, app, which_area);
  }

  if let Some(status_area) = status_area {
    draw_status(frame, app, status_area);
  } else {
    frame.render_widget(
      Block::default().style(Style::default().bg(theme.color(&theme.background))),
      area,
    );
  }
}

fn draw_prompt(frame: &mut Frame, app: &App, prompt: &Prompt, area: Rect) {
  let theme = &app.settings.theme;
  let style = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.background));
  let mut spans = vec![
    Span::styled(prompt_prefix(prompt), style.fg(theme.color(&theme.accent))),
    Span::styled(prompt_input(prompt), style),
  ];
  if matches!(prompt, Prompt::Command { .. })
    && prompt.buffer().cursor == prompt.buffer().input.len()
    && let Some(completion) = &app.command_completion
  {
    let suggestion = completion.suggestion_suffix();
    if !suggestion.is_empty() {
      spans.push(Span::styled(
        suggestion,
        style.fg(theme.color(&theme.muted)),
      ));
    }
  }
  frame.render_widget(Paragraph::new(Line::from(spans)).style(style), area);
  let input_width = prompt.buffer().cursor_columns() as u16;
  let prefix_width = prompt_prefix(prompt).chars().count() as u16;
  let cursor_x = prefix_width
    .saturating_add(input_width)
    .min(area.width.saturating_sub(1));
  frame.set_cursor_position((area.x.saturating_add(cursor_x), area.y));
}

fn draw_command_completion(frame: &mut Frame, app: &App, area: Rect) {
  let Some(completion) = &app.command_completion else {
    return;
  };
  if completion.candidates.is_empty() || area.height == 0 {
    return;
  }

  let theme = &app.settings.theme;
  let base = Style::default()
    .fg(theme.color(&theme.which_key_foreground))
    .bg(theme.color(&theme.which_key_background));
  frame.render_widget(Block::default().style(base), area);

  let visible = area.height as usize;
  let selected = completion.selected.min(completion.candidates.len() - 1);
  let start = selected.saturating_sub(visible.saturating_sub(1));
  let mut lines = Vec::with_capacity(visible);
  for row in 0..visible {
    let index = start + row;
    let Some(candidate) = completion.candidates.get(index) else {
      lines.push(Line::from(Span::styled(
        " ".repeat(area.width as usize),
        base,
      )));
      continue;
    };
    let selected_row = index == selected;
    let style = if selected_row {
      Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD)
    } else {
      base.fg(theme.color(&theme.which_key_foreground))
    };
    let marker = if selected_row { "> " } else { "  " };
    let mut text = format!("{marker}{candidate}");
    text = truncate_for_width(&text, area.width as usize);
    let used = text.chars().count();
    if used < area.width as usize {
      text.push_str(&" ".repeat(area.width as usize - used));
    }
    lines.push(Line::from(Span::styled(text, style)));
  }

  frame.render_widget(Paragraph::new(Text::from(lines)).style(base), area);
}

fn draw_which_key(frame: &mut Frame, app: &App, area: Rect) {
  if area.width == 0 || area.height == 0 {
    return;
  }

  let theme = &app.settings.theme;
  let base = Style::default()
    .fg(theme.color(&theme.which_key_foreground))
    .bg(theme.color(&theme.which_key_background));
  frame.render_widget(Block::default().style(base), area);

  let columns = which_key_columns(app, area.width);
  let rows = which_key_rows_for_count(app.hints.len(), columns).min(area.height);
  let cell_width = (area.width as usize / columns.max(1)).max(1);
  let mut lines = Vec::with_capacity(rows as usize);
  for row in 0..rows as usize {
    let mut spans = Vec::new();
    for col in 0..columns {
      let index = row * columns + col;
      if let Some(hint) = app.hints.get(index) {
        push_which_key_cell(&mut spans, hint, cell_width, app);
      } else if col + 1 < columns {
        spans.push(Span::styled(" ".repeat(cell_width), base));
      }
    }
    lines.push(Line::from(spans));
  }

  frame.render_widget(Paragraph::new(Text::from(lines)).style(base), area);
}

fn push_which_key_cell(
  spans: &mut Vec<Span<'static>>,
  hint: &KeyHint,
  cell_width: usize,
  app: &App,
) {
  let theme = &app.settings.theme;
  let base = Style::default()
    .fg(theme.color(&theme.which_key_foreground))
    .bg(theme.color(&theme.which_key_background));
  let key_style = base
    .fg(theme.color(&theme.which_key_key))
    .add_modifier(Modifier::BOLD);
  let separator_style = base.fg(theme.color(&theme.which_key_separator_color));
  let desc_style = base.fg(theme.color(&theme.which_key_description));

  let key = truncate_for_width(&hint.key, cell_width);
  let key_width = key.chars().count();
  spans.push(Span::styled(key, key_style));

  let mut used = key_width;
  if used < cell_width {
    let separator = truncate_for_width(&theme.which_key_separator, cell_width - used);
    used += separator.chars().count();
    spans.push(Span::styled(separator, separator_style));
  }
  if used < cell_width {
    let desc = truncate_for_width(&hint.label, cell_width - used);
    used += desc.chars().count();
    spans.push(Span::styled(desc, desc_style));
  }
  if used < cell_width {
    spans.push(Span::styled(" ".repeat(cell_width - used), base));
  }
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
  let theme = &app.settings.theme;
  let style = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.background));
  frame.render_widget(
    Paragraph::new(Line::from(vec![
      Span::styled(
        match app.view {
          ViewMode::Browser => "browser",
          ViewMode::Detail => detail_label(app.detail_page),
        },
        style.fg(theme.color(&theme.accent)),
      ),
      Span::styled(
        format!(
          "  {}/{}  selected:{}  sort:{}  {}",
          if app.images.is_empty() {
            0
          } else {
            app.focused + 1
          },
          app.images.len(),
          app.selected.len(),
          app.sort_spec.label(),
          app.message
        ),
        style,
      ),
    ]))
    .style(style),
    area,
  );
}

fn draw_confirm(frame: &mut Frame, app: &App, area: Rect) {
  let Some(confirm) = &app.confirm else {
    return;
  };
  if area.width < 20 || area.height < 6 {
    return;
  }
  let theme = &app.settings.theme;
  let available_width = area.width.saturating_sub(4).max(1);
  let width = available_width.min(96).max(available_width.min(40));
  let height = area.height.saturating_sub(2).min(12).max(6);
  let popup = Rect::new(
    area.x + area.width.saturating_sub(width) / 2,
    area.y + area.height.saturating_sub(height) / 2,
    width,
    height,
  );
  let style = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.which_key_background));
  frame.render_widget(Clear, popup);
  frame.render_widget(Block::default().style(style), popup);
  let text = match confirm {
    ConfirmDialog::MetadataWrite { path, edit } => {
      let mut lines = vec![
        Line::from(Span::styled(
          "Apply metadata changes?",
          style
            .fg(theme.color(&theme.accent))
            .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
          format!(
            "{} change(s): {}",
            edit.change_count(),
            display_file_name(path)
          ),
          style,
        )),
      ];
      if let Some(change) = &edit.file_name {
        lines.push(Line::from(Span::styled(
          format!("filename: {}", change.new_value),
          style,
        )));
      }
      for change in edit.tags.iter().take(4) {
        lines.push(Line::from(Span::styled(
          format!("{}: {}", change.tag, change.new_value),
          style,
        )));
      }
      if edit.tags.len() > 4 {
        lines.push(Line::from(Span::styled("...", style)));
      }
      lines.push(Line::from(Span::styled(
        "y apply    Enter/n/esc cancel",
        style.fg(theme.color(&theme.muted)),
      )));
      Text::from(lines)
    }
  };
  frame.render_widget(
    Paragraph::new(text)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .title("confirm")
          .border_style(style),
      )
      .style(style)
      .wrap(Wrap { trim: true }),
    popup,
  );
}

fn display_file_name(path: &std::path::Path) -> String {
  path
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.display().to_string())
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

fn metadata_line(label: &str, value: &str, theme: &crate::config::ThemeConfig) -> Line<'static> {
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

fn detail_label(page: DetailPage) -> &'static str {
  match page {
    DetailPage::Image => "image",
    DetailPage::Metadata => "metadata",
  }
}

fn prompt_prefix(prompt: &Prompt) -> &'static str {
  match prompt {
    Prompt::Rename { .. } => "rename: ",
    Prompt::Command { .. } => ":",
  }
}

fn prompt_input(prompt: &Prompt) -> String {
  prompt.buffer().input.clone()
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
  if layout.show_border {
    safe_inner(area, 1, 1)
  } else {
    area
  }
}
