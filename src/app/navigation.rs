use std::path::Path;

use crate::app::{App, DetailPage, ViewMode};

impl App {
  pub(super) fn handle_back(&mut self) {
    if self.view == ViewMode::Detail {
      if self.detail_back_quits {
        self.quit = true;
      } else {
        self.view = ViewMode::Browser;
      }
    } else {
      self.pending_keys.clear();
      self.hints.clear();
    }
  }

  pub(super) fn move_left(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_horizontal(-1),
      ViewMode::Detail => self.detail_page = DetailPage::Image,
    }
  }

  pub(super) fn move_right(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_horizontal(1),
      ViewMode::Detail => self.detail_page = DetailPage::Metadata,
    }
  }

  pub(super) fn move_down(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(1),
      ViewMode::Detail => self.focus_relative(1),
    }
  }

  pub(super) fn move_up(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(-1),
      ViewMode::Detail => self.focus_relative(-1),
    }
  }

  pub(super) fn page_up(&mut self) {
    match self.view {
      ViewMode::Browser => {
        let count = self.page_item_count();
        self.focus_relative(-(count as isize));
      }
      ViewMode::Detail => self.focus_relative(-1),
    }
  }

  pub(super) fn page_down(&mut self) {
    match self.view {
      ViewMode::Browser => {
        let count = self.page_item_count();
        self.focus_relative(count as isize);
      }
      ViewMode::Detail => self.focus_relative(1),
    }
  }

  pub(super) fn focus_first(&mut self) {
    self.focused = 0;
    self.ensure_focus_visible();
  }

  pub(super) fn focus_last(&mut self) {
    if !self.images.is_empty() {
      self.focused = self.images.len() - 1;
      self.ensure_focus_visible();
    }
  }

  pub(super) fn toggle_select(&mut self) {
    let Some(path) = self.current().map(|item| item.path.clone()) else {
      return;
    };
    if !self.selected.remove(&path) {
      self.selected.insert(path);
    }
    if self.settings.config.behavior.select_moves_focus && self.focused + 1 < self.images.len() {
      self.focused += 1;
      self.ensure_focus_visible();
    }
  }

  pub(super) fn clear_selection(&mut self) {
    let count = self.selected.len();
    if count == 0 {
      self.set_message("no selected images");
      return;
    }
    self.selected.clear();
    self.set_message(format!("cleared {count} selected image(s)"));
  }

  pub(super) fn handle_scroll_down(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(1),
      ViewMode::Detail => self.focus_relative(1),
    }
  }

  pub(super) fn handle_scroll_up(&mut self) {
    match self.view {
      ViewMode::Browser => self.focus_vertical(-1),
      ViewMode::Detail => self.focus_relative(-1),
    }
  }

  pub(super) fn handle_mouse_click(&mut self, column: u16, row: u16) {
    if self.view != ViewMode::Browser || self.images.is_empty() {
      return;
    }
    let Some(layout) = &self.last_layout else {
      return;
    };
    let Some(viewport) = self.browser_viewport else {
      return;
    };
    if column < viewport.x
      || row < viewport.y
      || column >= viewport.x.saturating_add(viewport.width)
      || row >= viewport.y.saturating_add(viewport.height)
    {
      return;
    }

    let canvas_x = column.saturating_sub(viewport.x);
    let canvas_y = u32::from(row.saturating_sub(viewport.y)).saturating_add(self.browser_scroll);
    let target = layout.cards.iter().enumerate().find_map(|(index, card)| {
      let within_x = canvas_x >= card.x && canvas_x < card.x.saturating_add(card.width);
      let within_y = canvas_y >= card.y && canvas_y < card.y.saturating_add(u32::from(card.height));
      if within_x && within_y {
        Some(index)
      } else {
        None
      }
    });

    if let Some(index) = target.filter(|index| *index < self.images.len()) {
      self.focused = index;
      self.pending_keys.clear();
      self.hints.clear();
      self.ensure_focus_visible();
    }
  }

  pub(super) fn ensure_focus_visible(&mut self) {
    if self.view != ViewMode::Browser {
      return;
    }
    let Some(layout) = &self.last_layout else {
      return;
    };
    let Some(card) = layout.cards.get(self.focused) else {
      return;
    };
    let top = card.y;
    let bottom = card.y.saturating_add(u32::from(card.height));
    let viewport_height = u32::from(self.browser_view_height.max(1));
    let visible_bottom = self.browser_scroll.saturating_add(viewport_height);
    if u32::from(card.height) >= viewport_height {
      if bottom <= self.browser_scroll || top >= visible_bottom {
        self.browser_scroll = top;
      }
      return;
    }

    if top < self.browser_scroll {
      self.browser_scroll = top;
    } else if bottom > visible_bottom {
      self.browser_scroll = bottom.saturating_sub(viewport_height);
    }
  }

  pub(super) fn restore_focus(&mut self, path: Option<&Path>) {
    if let Some(path) = path
      && let Some(idx) = self.images.iter().position(|item| item.path == path)
    {
      self.focused = idx;
      return;
    }
    if self.images.is_empty() {
      self.focused = 0;
    } else {
      self.focused = self.focused.min(self.images.len() - 1);
    }
  }

  fn focus_relative(&mut self, delta: isize) {
    if self.images.is_empty() {
      self.focused = 0;
      return;
    }
    let max = self.images.len() - 1;
    self.focused = self.focused.saturating_add_signed(delta).min(max);
    self.ensure_focus_visible();
  }

  fn focus_horizontal(&mut self, delta: isize) {
    let Some(layout) = &self.last_layout else {
      self.focus_relative(delta);
      return;
    };
    if layout.columns <= 1 {
      self.focus_relative(delta);
      return;
    }
    let col = self.focused % layout.columns;
    if delta < 0 && col == 0 {
      return;
    }
    if delta > 0 && col + 1 >= layout.columns {
      return;
    }
    self.focus_relative(delta);
  }

  fn focus_vertical(&mut self, delta: isize) {
    let Some(layout) = &self.last_layout else {
      self.focus_relative(delta);
      return;
    };
    if layout.cards.len() != self.images.len() || self.focused >= layout.cards.len() {
      self.focus_relative(delta * layout.columns as isize);
      return;
    }

    let current = layout.cards[self.focused];
    let target = layout
      .cards
      .iter()
      .enumerate()
      .filter(|(idx, card)| {
        if delta > 0 {
          *idx != self.focused && card.y > current.y
        } else {
          *idx != self.focused && card.y < current.y
        }
      })
      .min_by_key(|(_, card)| {
        let dy = card.y.abs_diff(current.y);
        let dx = (card.center_x() - current.center_x()).unsigned_abs();
        dy.saturating_mul(1000).saturating_add(dx)
      })
      .map(|(idx, _)| idx);
    if let Some(idx) = target {
      self.focused = idx;
      self.ensure_focus_visible();
    }
  }

  fn page_item_count(&self) -> usize {
    let Some(layout) = &self.last_layout else {
      return 1;
    };
    let viewport_top = self.browser_scroll;
    let viewport_bottom = self
      .browser_scroll
      .saturating_add(u32::from(self.browser_view_height));
    layout
      .cards
      .iter()
      .filter(|card| {
        let card_top = card.y;
        let card_bottom = card.y.saturating_add(u32::from(card.height));
        card_bottom > viewport_top && card_top < viewport_bottom
      })
      .count()
      .max(1)
  }
}
