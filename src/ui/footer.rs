use framework_tui::{
  CompletionListStyle, KeyHintsStyle, PromptLineStyle, completion_rows,
  default_completion_selected_style, draw_completion_list, draw_key_hints, draw_prompt_line,
  key_hint_columns, key_hint_rows,
};
use ratatui::{
  Frame,
  layout::Rect,
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Paragraph},
};

use crate::app::{App, DetailPage, Prompt, ViewMode};

pub(super) fn footer_height(app: &App, width: u16) -> u16 {
  let status = u16::from(status_visible(app));
  let prompt = u16::from(app.prompt.is_some());
  let completion = if app.prompt.is_some() {
    completion_rows(app.command_completion(), 5)
  } else {
    0
  };
  let hints = app.key_hints();
  let which = if hints.is_empty() {
    0
  } else {
    key_hint_rows(hints.len(), which_key_columns(app, width))
  };
  status
    .saturating_add(prompt)
    .saturating_add(completion)
    .saturating_add(which)
}

pub(super) fn draw_footer(
  frame: &mut Frame,
  app: &App,
  area: Rect,
  cursor_position: &mut Option<(u16, u16)>,
) {
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
    draw_prompt(frame, app, prompt, prompt_area, cursor_position);
  }

  let completion_rows = command_completion_rows(app);
  if completion_rows > 0 && content_bottom > area.y {
    let height = completion_rows.min(content_bottom - area.y);
    content_bottom = content_bottom.saturating_sub(height);
    let completion_area = Rect::new(area.x, content_bottom, area.width, height);
    draw_command_completion(frame, app, completion_area);
  }

  if !app.key_hints().is_empty() && area.y < content_bottom {
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
  if app.prompt.is_some() {
    completion_rows(app.command_completion(), 5)
  } else {
    0
  }
}

fn status_visible(app: &App) -> bool {
  !(app.view == ViewMode::Detail
    && app.detail_page == DetailPage::Image
    && app.prompt.is_none()
    && app.key_hints().is_empty())
}

fn which_key_columns(app: &App, width: u16) -> usize {
  key_hint_columns(app.settings.theme.which_key_columns as usize, width)
}

fn draw_prompt(
  frame: &mut Frame,
  app: &App,
  prompt: &Prompt,
  area: Rect,
  cursor_position: &mut Option<(u16, u16)>,
) {
  let theme = &app.settings.theme;
  let base = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.background));
  let style = PromptLineStyle {
    base,
    prefix: base.fg(theme.color(&theme.accent)),
    suggestion: base.fg(theme.color(&theme.muted)),
  };
  if let Some(position) = draw_prompt_line(frame, prompt, app.command_completion(), area, &style) {
    *cursor_position = Some(position);
  }
}

fn draw_command_completion(frame: &mut Frame, app: &App, area: Rect) {
  let Some(completion) = app.command_completion() else {
    return;
  };
  let theme = &app.settings.theme;
  let base = Style::default()
    .fg(theme.color(&theme.which_key_foreground))
    .bg(theme.color(&theme.which_key_background));
  let style = CompletionListStyle {
    base,
    selected: default_completion_selected_style(),
  };
  draw_completion_list(frame, completion, area, &style);
}

fn draw_which_key(frame: &mut Frame, app: &App, area: Rect) {
  let theme = &app.settings.theme;
  let base = Style::default()
    .fg(theme.color(&theme.which_key_foreground))
    .bg(theme.color(&theme.which_key_background));
  let style = KeyHintsStyle {
    base,
    key: base
      .fg(theme.color(&theme.which_key_key))
      .add_modifier(Modifier::BOLD),
    separator: base.fg(theme.color(&theme.which_key_separator_color)),
    description: base.fg(theme.color(&theme.which_key_description)),
    separator_text: theme.which_key_separator.clone(),
    columns: theme.which_key_columns as usize,
  };
  draw_key_hints(frame, app.key_hints(), area, &style);
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
