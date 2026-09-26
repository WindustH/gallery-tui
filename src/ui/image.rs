use img_tui::ProtocolOverlay;
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
  config::EffectiveLayoutConfig,
  event::AsyncEvent,
  model::ImageItem,
  render::{RenderState, RenderStore, RenderedImage},
};

/// Cell size assumed when the terminal does not report one.
const DEFAULT_CELL_PIXELS: (u16, u16) = (8, 16);

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

/// Image rendering state for one frame: the render store plus the protocol
/// overlays collected while drawing.
pub(super) struct FrameImages<'a> {
  renderer: &'a mut RenderStore,
  tx: &'a mpsc::UnboundedSender<AsyncEvent>,
  cell_pixels: Option<(u16, u16)>,
  pub(super) overlays: Vec<ProtocolOverlay>,
  /// Keep previous protocol images visible where new ones are still pending.
  pub(super) preserve_overlays: bool,
  pub(super) preserve_areas: Vec<Rect>,
}

impl<'a> FrameImages<'a> {
  pub(super) fn new(
    renderer: &'a mut RenderStore,
    tx: &'a mpsc::UnboundedSender<AsyncEvent>,
    cell_pixels: Option<(u16, u16)>,
  ) -> Self {
    renderer.begin_frame();
    Self {
      renderer,
      tx,
      cell_pixels,
      overlays: Vec::new(),
      preserve_overlays: false,
      preserve_areas: Vec::new(),
    }
  }

  /// Draw `item` fitted inside `area`, requesting a render when needed.
  /// `position` is the item's (index, total) shown while it renders.
  pub(super) fn draw(
    &mut self,
    frame: &mut Frame,
    item: &ImageItem,
    area: Rect,
    alignment: ImageAlignment,
    position: (usize, usize),
  ) {
    if area.width == 0 || area.height == 0 {
      return;
    }
    let image_area = fit_image_rect(area, item, self.cell_pixels, alignment);
    if image_area.width == 0 || image_area.height == 0 {
      return;
    }

    let draws_with_protocol = self.renderer.draws_with_protocol();
    match self
      .renderer
      .request(item, image_area.width, image_area.height, self.tx)
    {
      RenderState::Ready(RenderedImage::Symbols { paragraph, .. }) => {
        frame.render_widget(&**paragraph, image_area);
      }
      RenderState::Ready(RenderedImage::Protocol(image)) => {
        reserve_protocol_area(frame, image_area);
        self.overlays.push(image.overlay(image_area));
      }
      RenderState::Failed(error) => {
        frame.render_widget(
          Paragraph::new(format!("render failed\n{error}")).wrap(Wrap { trim: true }),
          image_area,
        );
      }
      RenderState::Pending if draws_with_protocol => {
        self.preserve_overlays = true;
        self.preserve_areas.push(image_area);
      }
      RenderState::Pending => {
        frame.render_widget(
          Paragraph::new(rendering_text(item, position))
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::DIM)),
          image_area,
        );
      }
    }
  }

  /// Start a background render of `item` as it would be drawn in `area`.
  pub(super) fn preload(&mut self, item: &ImageItem, area: Rect, alignment: ImageAlignment) {
    let fitted = fit_image_rect(area, item, self.cell_pixels, alignment);
    self
      .renderer
      .preload(item, fitted.width, fitted.height, self.tx);
  }
}

/// The largest rect inside `area` with the image's aspect ratio, measured in
/// terminal cells of `cell_pixels` size.
pub(super) fn fit_image_rect(
  area: Rect,
  item: &ImageItem,
  cell_pixels: Option<(u16, u16)>,
  alignment: ImageAlignment,
) -> Rect {
  let Some((image_width, image_height)) = item.dimensions else {
    return area;
  };
  if image_width == 0 || image_height == 0 || area.width == 0 || area.height == 0 {
    return area;
  }

  let (cell_width, cell_height) = cell_pixels.unwrap_or(DEFAULT_CELL_PIXELS);
  let cell_width = f64::from(cell_width.max(1));
  let cell_height = f64::from(cell_height.max(1));
  let max_pixel_width = f64::from(area.width) * cell_width;
  let max_pixel_height = f64::from(area.height) * cell_height;
  let scale = (max_pixel_width / f64::from(image_width))
    .min(max_pixel_height / f64::from(image_height))
    .max(0.0);

  let fitted_width = ((f64::from(image_width) * scale) / cell_width)
    .round()
    .clamp(1.0, f64::from(area.width)) as u16;
  let fitted_height = ((f64::from(image_height) * scale) / cell_height)
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

fn rendering_text(item: &ImageItem, (index, total): (usize, usize)) -> Text<'static> {
  Text::from(vec![
    Line::from("rendering..."),
    Line::from(item.file_name.clone()),
    Line::from(format!("({}/{})", index.saturating_add(1), total.max(1))),
  ])
}

/// Leave cells under a protocol image out of the text diff so ratatui does
/// not paint over the image.
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
