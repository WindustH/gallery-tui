use framework_tui::{PopupDialogStyle, draw_popup_dialog};
use ratatui::{
  Frame,
  layout::Rect,
  style::{Modifier, Style},
  text::{Line, Span, Text},
};

use crate::app::{App, ConfirmDialog};

pub(super) fn draw_confirm(frame: &mut Frame, app: &App, area: Rect) {
  let Some(confirm) = &app.confirm else {
    return;
  };
  let theme = &app.settings.theme;
  let style = Style::default()
    .fg(theme.color(&theme.foreground))
    .bg(theme.color(&theme.which_key_background));
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
  let popup_style = PopupDialogStyle {
    base: style,
    border: style,
    ..PopupDialogStyle::default()
  };
  let _ = draw_popup_dialog(frame, area, "confirm", text, &popup_style);
}

fn display_file_name(path: &std::path::Path) -> String {
  path
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.display().to_string())
}
