use std::thread;

use anyhow::Result;
use framework_tui::{
  SuspendTerminal, TerminalOptions, TerminalOutput, TerminalSession, install_panic_hook,
};
use img_tui::{ProtocolFrameOutput, ProtocolFrameRenderer};
use ratatui::Frame;

pub type FrameOutput = ProtocolFrameOutput;

/// The framework-tui session plus the protocol images drawn on it, which
/// are erased before the terminal is handed back.
pub struct Tui {
  session: TerminalSession,
  protocol_renderer: ProtocolFrameRenderer,
}

impl Tui {
  pub fn new() -> Result<Self> {
    // Every panic while the UI is up is logged. A panic on the UI thread
    // then resets the terminal so the message is readable; worker panics
    // become failed jobs, and printing them would corrupt the screen.
    install_panic_hook(|info, _| {
      tracing::error!(thread = thread::current().name(), %info, "panic");
    });
    let session = TerminalSession::enter(TerminalOptions {
      output: TerminalOutput::Stderr,
      buffer_capacity: 0,
      ..TerminalOptions::default()
    })?;
    Ok(Self {
      session,
      protocol_renderer: ProtocolFrameRenderer::default(),
    })
  }

  pub fn draw<F>(&mut self, render: F) -> Result<()>
  where
    F: FnOnce(&mut Frame) -> FrameOutput,
  {
    self
      .protocol_renderer
      .draw(self.session.terminal_mut(), render)
  }

  pub fn restore(&mut self) -> Result<()> {
    // Attempt every step even if an earlier one fails, so the shell gets
    // its terminal back in as usable a state as possible.
    let cleared = self.clear_images();
    let session = self.session.restore();
    cleared.and(session.map_err(Into::into))
  }

  fn clear_images(&mut self) -> Result<()> {
    if self.session.is_suspended() || self.session.is_restored() {
      return Ok(());
    }
    self.protocol_renderer.clear(self.session.backend_mut())
  }
}

impl SuspendTerminal for Tui {
  type Error = anyhow::Error;

  /// Hand the terminal to another program, such as an external editor.
  fn suspend(&mut self) -> Result<()> {
    let cleared = self.clear_images();
    let session = self.session.suspend();
    cleared.and(session.map_err(Into::into))
  }

  fn resume(&mut self) -> Result<()> {
    Ok(self.session.resume()?)
  }
}

impl Drop for Tui {
  /// Restore the terminal when `main` returns early with an error. During a
  /// panic the hook has already reset the terminal modes.
  fn drop(&mut self) {
    if thread::panicking() {
      return;
    }
    if let Err(error) = self.restore() {
      tracing::warn!(%error, "failed to restore terminal");
    }
  }
}
