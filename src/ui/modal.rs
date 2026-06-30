use ratatui::{
  Frame,
  layout::Rect,
  style::{Modifier, Style},
  text::{Line, Span, Text},
  widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{App, ConfirmDialog};

pub(super) fn draw_confirm(frame: &mut Frame, app: &App, area: Rect) {
  let Some(confirm) = &app.confirm else {
    return;
  };
  if area.width < 20 || area.height < 6 {
    return;
  }
  let theme = &app.settings.theme;
  let available_width = area.width.saturating_sub(4).max(1);
  let width = available_width.min(96).max(available_width.min(40));
  let height = area.height.saturating_sub(2).clamp(6, 12);
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
