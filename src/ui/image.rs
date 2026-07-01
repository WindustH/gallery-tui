use ratatui::{
  Frame,
  buffer::CellDiffOption,
  layout::{Alignment, Rect},
  style::{Modifier, Style},
  text::{Line, Text},
  widgets::{Paragraph, Wrap},
};
use tokio::sync::mpsc;

use crate::{
  app::App,
  config::EffectiveLayoutConfig,
  event::{AsyncEvent, ProtocolOverlay, RenderedImage},
  model::ImageItem,
  render::RenderStore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImageAlignment {
  Left,
  Center,
}

pub(super) fn image_alignment_for_layout(layout: &EffectiveLayoutConfig) -> ImageAlignment {
  match layout.image_alignment.as_str() {
    "left" => ImageAlignment::Left,
    _ => ImageAlignment::Center,
  }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_rendered_image(
  frame: &mut Frame,
  app: &App,
  renderer: &mut RenderStore,
  tx: &mpsc::UnboundedSender<AsyncEvent>,
  item: &ImageItem,
  index: usize,
  total: usize,
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
        placement,
        fingerprint,
        erase,
      } => {
        let _mode_label = mode.label();
        reserve_protocol_area(frame, image_area);
        protocol_overlays.push(ProtocolOverlay {
          area: image_area,
          mode: *mode,
          data: data.clone(),
          placement: placement.clone(),
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
      Paragraph::new(rendering_text(item, index, total))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM)),
      image_area,
    );
  }
}

pub(super) fn fit_image_rect(
  area: Rect,
  item: &ImageItem,
  app: &App,
  alignment: ImageAlignment,
) -> Rect {
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

fn rendering_text(item: &ImageItem, index: usize, total: usize) -> Text<'static> {
  Text::from(vec![
    Line::from("rendering..."),
    Line::from(item.file_name.clone()),
    Line::from(format!("({}/{})", index.saturating_add(1), total.max(1))),
  ])
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
