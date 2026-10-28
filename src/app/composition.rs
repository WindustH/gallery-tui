//! Layout-independent reading position and a follower panel's geometry.
use super::App;
use crate::{
  config::EffectiveLayoutConfig,
  layout::{BrowserLayout, compute_browser_layout, screen_rect},
};
use ratatui::layout::Rect;

/// A position inside an image, independent of rows, columns, or panel dimensions.
#[derive(Clone, Copy)]
pub(crate) struct ImageProgress {
  pub index: usize,
  pub fraction: f64,
}

pub(crate) struct BrowserPanel {
  pub config: EffectiveLayoutConfig,
  pub layout: BrowserLayout,
  pub area: Rect,
  pub scroll: u32,
  cell_pixels: Option<(u16, u16)>,
}

impl App {
  pub(crate) fn image_progress(&self) -> ImageProgress {
    progress_at(
      self.last_layout.as_ref(),
      self.focused,
      self.browser_scroll,
      self.browser_view_height,
    )
  }

  /// Predict follower positions by navigating the primary, not by moving
  /// through the follower's unrelated row/column arrangement.
  pub(crate) fn follower_preload_scrolls(&self) -> Vec<u32> {
    let (Some(primary), Some(panel)) = (&self.last_layout, &self.follower) else {
      return Vec::new();
    };
    let render = &self.settings.config.render;
    let config = self.settings.config.layout.effective();
    let mut down = Some((self.focused, self.browser_scroll));
    let mut up = down;
    let mut positions = Vec::new();
    for distance in 1..=render
      .preload_ahead
      .max(render.preload_behind)
      .min(self.images.len())
    {
      for (route, forward, count) in [
        (&mut down, true, render.preload_ahead),
        (&mut up, false, render.preload_behind),
      ] {
        if distance > count {
          continue;
        }
        if let Some((focused, scroll)) = *route {
          *route = primary.vertical_target(focused, forward).map(|next| {
            let scroll =
              primary.scroll_for_focus(next, scroll, self.browser_view_height, &config.strategy);
            positions.push(panel.scroll_for_progress(progress_at(
              Some(primary),
              next,
              scroll,
              self.browser_view_height,
            )));
            (next, scroll)
          });
        }
      }
    }
    positions
  }

  pub(crate) fn prepare_follower(&mut self, config: EffectiveLayoutConfig, area: Rect) {
    let progress = self.image_progress();
    if self.follower.as_ref().is_none_or(|p| {
      p.config != config || p.area != area || p.cell_pixels != self.terminal_cell_pixels
    }) {
      let layout = compute_browser_layout(&self.images, area, &config, self.terminal_cell_pixels);
      self.follower = Some(BrowserPanel {
        config,
        layout,
        area,
        scroll: 0,
        cell_pixels: self.terminal_cell_pixels,
      });
    }
    let panel = self.follower.as_mut().unwrap();
    panel.scroll = panel.scroll_for_progress(progress);
  }

  pub(super) fn follower_image_at(&self, column: u16, row: u16) -> Option<usize> {
    let panel = self.follower.as_ref()?;
    panel
      .layout
      .cards
      .iter()
      .enumerate()
      .find_map(|(index, card)| {
        let area = screen_rect(*card, panel.area, panel.scroll)?;
        if panel.config.strategy != "masonry"
          && (card.y < panel.scroll || area.height != card.height)
        {
          return None;
        }
        area.contains((column, row).into()).then_some(index)
      })
  }
}

impl BrowserPanel {
  fn scroll_for_progress(&self, progress: ImageProgress) -> u32 {
    let layout = &self.layout;
    let area = self.area;
    let config = &self.config;
    layout.cards.get(progress.index).map_or(0, |card| {
      let target = (f64::from(card.y) + progress.fraction * f64::from(card.height)
        - f64::from(area.height) / 2.0)
        .max(0.0) as u32;
      if config.strategy == "masonry" {
        target.min(layout.total_height.saturating_sub(u32::from(area.height)))
      } else {
        let stride = layout
          .cards
          .get(layout.columns)
          .map_or(u32::from(card.height), |next| next.y)
          .max(1);
        layout.scroll_for_focus(
          progress.index,
          target / stride * stride,
          area.height,
          &config.strategy,
        )
      }
    })
  }
}

fn progress_at(
  layout: Option<&BrowserLayout>,
  index: usize,
  scroll: u32,
  height: u16,
) -> ImageProgress {
  let fraction = layout
    .and_then(|layout| layout.cards.get(index))
    .map_or(0.5, |card| {
      let top = card.y.max(scroll);
      let bottom = card
        .y
        .saturating_add(u32::from(card.height))
        .min(scroll.saturating_add(u32::from(height)));
      if bottom <= top {
        0.5
      } else {
        ((f64::from(top) + f64::from(bottom)) / 2.0 - f64::from(card.y))
          / f64::from(card.height.max(1))
      }
    });
  ImageProgress {
    index,
    fraction: fraction.clamp(0.0, 1.0),
  }
}
