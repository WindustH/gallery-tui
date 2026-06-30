use ratatui::{
  Frame,
  layout::Rect,
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{Block, Paragraph},
};

use crate::{
  app::{App, DetailPage, Prompt, ViewMode},
  keymap::KeyHint,
};

use super::truncate_for_width;

pub(super) fn footer_height(app: &App, width: u16) -> u16 {
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

pub(super) fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
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
