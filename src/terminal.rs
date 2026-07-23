use std::io::{self, Stderr};

use anyhow::Result;
use crossterm::{
  event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use img_tui::{ProtocolFrameOutput, ProtocolFrameRenderer};
use ratatui::{Frame, Terminal, prelude::CrosstermBackend};

pub type FrameOutput = ProtocolFrameOutput;

pub struct Tui {
  terminal: Terminal<CrosstermBackend<Stderr>>,
  protocol_renderer: ProtocolFrameRenderer,
  suspended: bool,
  restored: bool,
}

impl Tui {
  pub fn new() -> Result<Self> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(
      stderr,
      EnterAlternateScreen,
      EnableMouseCapture,
      EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stderr);
    let terminal = Terminal::new(backend)?;
    Ok(Self {
      terminal,
      protocol_renderer: ProtocolFrameRenderer::default(),
      suspended: false,
      restored: false,
    })
  }

  pub fn draw<F>(&mut self, render: F) -> Result<()>
  where
    F: FnOnce(&mut Frame) -> FrameOutput,
  {
    self.protocol_renderer.draw(&mut self.terminal, render)
  }

  pub fn restore(&mut self) -> Result<()> {
    if self.restored {
      return Ok(());
    }
    let backend = self.terminal.backend_mut();
    self.protocol_renderer.clear(backend)?;
    disable_raw_mode()?;
    self.terminal.show_cursor()?;
    if !self.suspended {
      execute!(
        self.terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
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
    let backend = self.terminal.backend_mut();
    self.protocol_renderer.clear(backend)?;
    disable_raw_mode()?;
    self.terminal.show_cursor()?;
    execute!(
      self.terminal.backend_mut(),
      LeaveAlternateScreen,
      DisableMouseCapture,
      DisableBracketedPaste
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
      EnableMouseCapture,
      EnableBracketedPaste
    )?;
    self.terminal.clear()?;
    self.suspended = false;
    Ok(())
  }
}
