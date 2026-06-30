use std::io::{self, Stderr, Write};

use anyhow::Result;
use crossterm::{
  cursor::{MoveTo, RestorePosition, SavePosition},
  event::{DisableMouseCapture, EnableMouseCapture},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Frame, Terminal, prelude::CrosstermBackend};

use crate::{capability::RenderMode, event::ProtocolOverlay};

pub struct Tui {
  terminal: Terminal<CrosstermBackend<Stderr>>,
  protocol_state: Vec<ProtocolOverlayState>,
  suspended: bool,
  restored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProtocolOverlayState {
  area: ratatui::layout::Rect,
  mode: RenderMode,
  fingerprint: u64,
  erase: Option<String>,
}

impl Tui {
  pub fn new() -> Result<Self> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stderr);
    let terminal = Terminal::new(backend)?;
    Ok(Self {
      terminal,
      protocol_state: Vec::new(),
      suspended: false,
      restored: false,
    })
  }

  pub fn draw<F>(&mut self, render: F) -> Result<()>
  where
    F: FnOnce(&mut Frame),
  {
    self.terminal.draw(render)?;
    Ok(())
  }

  pub fn render_protocol_overlays(&mut self, overlays: &[ProtocolOverlay]) -> Result<()> {
    let next = overlays
      .iter()
      .map(|overlay| {
        (
          ProtocolOverlayState {
            area: overlay.area,
            mode: overlay.mode,
            fingerprint: overlay.fingerprint,
            erase: overlay.erase.clone(),
          },
          overlay,
        )
      })
      .collect::<Vec<_>>();
    let next_state = next
      .iter()
      .map(|(state, _)| state.clone())
      .collect::<Vec<_>>();

    if next_state == self.protocol_state {
      return Ok(());
    }

    let removed = self
      .protocol_state
      .iter()
      .filter(|state| !next_state.contains(state))
      .cloned()
      .collect::<Vec<_>>();
    let added = next
      .iter()
      .filter(|(state, _)| !self.protocol_state.contains(state))
      .map(|(_, overlay)| *overlay)
      .collect::<Vec<_>>();

    let backend = self.terminal.backend_mut();
    erase_protocol_state(backend, &removed)?;
    for overlay in added {
      clear_protocol_area(backend, overlay.area)?;
      execute!(
        backend,
        SavePosition,
        MoveTo(overlay.area.x, overlay.area.y)
      )?;
      backend.write_all(overlay.data.as_bytes())?;
      backend.write_all(b"\x1b[0m")?;
      execute!(backend, RestorePosition)?;
    }
    backend.flush()?;
    self.protocol_state = next_state;
    Ok(())
  }

  pub fn clear_protocol_overlays(&mut self) -> Result<()> {
    let old_state = std::mem::take(&mut self.protocol_state);
    if old_state.is_empty() {
      return Ok(());
    }
    let backend = self.terminal.backend_mut();
    erase_protocol_state(backend, &old_state)?;
    for overlay in old_state {
      clear_protocol_area(backend, overlay.area)?;
    }
    backend.flush()?;
    Ok(())
  }

  pub fn restore(&mut self) -> Result<()> {
    if self.restored {
      return Ok(());
    }
    let old_state = std::mem::take(&mut self.protocol_state);
    erase_protocol_state(self.terminal.backend_mut(), &old_state)?;
    disable_raw_mode()?;
    self.terminal.show_cursor()?;
    if !self.suspended {
      execute!(
        self.terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
      )?;
    }
    self.suspended = true;
    self.restored = true;
    Ok(())
  }

  pub fn suspend(&mut self) -> Result<()> {
    if self.suspended {
      return Ok(());
    }
    let old_state = std::mem::take(&mut self.protocol_state);
    erase_protocol_state(self.terminal.backend_mut(), &old_state)?;
    disable_raw_mode()?;
    self.terminal.show_cursor()?;
    execute!(
      self.terminal.backend_mut(),
      LeaveAlternateScreen,
      DisableMouseCapture
    )?;
    self.suspended = true;
    Ok(())
  }

  pub fn resume(&mut self) -> Result<()> {
    if !self.suspended {
      return Ok(());
    }
    enable_raw_mode()?;
    execute!(
      self.terminal.backend_mut(),
      EnterAlternateScreen,
      EnableMouseCapture
    )?;
    self.terminal.clear()?;
    self.suspended = false;
    Ok(())
  }
}

fn erase_protocol_state(
  backend: &mut CrosstermBackend<Stderr>,
  state: &[ProtocolOverlayState],
) -> Result<()> {
  if state.is_empty() {
    return Ok(());
  }

  execute!(backend, SavePosition)?;
  for overlay in state {
    if let Some(sequence) = &overlay.erase {
      backend.write_all(sequence.as_bytes())?;
    }
  }
  execute!(backend, RestorePosition)?;
  backend.flush()?;
  Ok(())
}

fn clear_protocol_area(
  backend: &mut CrosstermBackend<Stderr>,
  area: ratatui::layout::Rect,
) -> Result<()> {
  if area.width == 0 || area.height == 0 {
    return Ok(());
  }
  let blank = " ".repeat(area.width as usize);
  execute!(backend, SavePosition)?;
  for y in area.y..area.y.saturating_add(area.height) {
    execute!(backend, MoveTo(area.x, y))?;
    backend.write_all(blank.as_bytes())?;
  }
  execute!(backend, RestorePosition)?;
  Ok(())
}

impl Drop for Tui {
  fn drop(&mut self) {
    let _ = self.restore();
  }
}
