use std::{
  io::{self, Stderr},
  sync::{
    Arc, Once,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  thread,
  time::Duration,
};

use anyhow::Result;
use crossterm::{
  cursor::Show,
  event::{
    self as crossterm_event, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
    EnableMouseCapture,
  },
  execute,
  terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
  },
};
use img_tui::{ProtocolFrameOutput, ProtocolFrameRenderer};
use ratatui::{Frame, Terminal, prelude::CrosstermBackend};
use tokio::sync::mpsc;

use crate::event::AsyncEvent;

pub type FrameOutput = ProtocolFrameOutput;

/// Set while a `Tui` owns the terminal, so the panic hook knows it has to
/// restore it.
static TUI_ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct Tui {
  terminal: Terminal<CrosstermBackend<Stderr>>,
  protocol_renderer: ProtocolFrameRenderer,
  suspended: bool,
  restored: bool,
}

impl Tui {
  pub fn new() -> Result<Self> {
    install_panic_hook();
    enable_raw_mode()?;
    TUI_ACTIVE.store(true, Ordering::SeqCst);
    let terminal = (|| -> Result<_> {
      let mut stderr = io::stderr();
      execute!(
        stderr,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
      )?;
      Ok(Terminal::new(CrosstermBackend::new(stderr))?)
    })()
    .inspect_err(|_| reset_terminal_modes())?;
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
    self.restored = true;
    TUI_ACTIVE.store(false, Ordering::SeqCst);
    // Attempt every step even if an earlier one fails, so the shell gets
    // its terminal back in as usable a state as possible.
    let cleared = self.protocol_renderer.clear(self.terminal.backend_mut());
    let raw_mode = disable_raw_mode();
    let cursor = self.terminal.show_cursor();
    let screen = if self.suspended {
      Ok(())
    } else {
      self.leave_alternate_screen()
    };
    self.suspended = true;
    cleared?;
    raw_mode?;
    cursor?;
    screen
  }

  /// Hand the terminal to another program, such as an external editor.
  pub fn suspend(&mut self) -> Result<()> {
    if self.suspended {
      return Ok(());
    }
    let backend = self.terminal.backend_mut();
    self.protocol_renderer.clear(backend)?;
    disable_raw_mode()?;
    self.terminal.show_cursor()?;
    self.leave_alternate_screen()?;
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
      EnableBracketedPaste,
      Clear(ClearType::All)
    )?;
    // Repaint everything on the next frame. `Terminal::clear` would do this
    // too, but it first queries the cursor position through stdout, which
    // leaks the query into piped output and fails when stdout is not the
    // terminal.
    self.terminal.current_buffer_mut().reset();
    self.terminal.swap_buffers();
    self.suspended = false;
    Ok(())
  }

  fn leave_alternate_screen(&mut self) -> Result<()> {
    execute!(
      self.terminal.backend_mut(),
      LeaveAlternateScreen,
      DisableMouseCapture,
      DisableBracketedPaste
    )?;
    Ok(())
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

/// Best-effort terminal reset for paths that cannot reach the `Tui`.
fn reset_terminal_modes() {
  TUI_ACTIVE.store(false, Ordering::SeqCst);
  let _ = disable_raw_mode();
  let _ = execute!(
    io::stderr(),
    LeaveAlternateScreen,
    DisableMouseCapture,
    DisableBracketedPaste,
    Show
  );
}

fn install_panic_hook() {
  static INSTALL: Once = Once::new();
  INSTALL.call_once(|| {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
      if !TUI_ACTIVE.load(Ordering::SeqCst) {
        default_hook(info);
        return;
      }
      tracing::error!(thread = thread::current().name(), %info, "panic");
      if thread::current().name() == Some("main") {
        // The UI loop is gone: reset the terminal so the message is readable.
        reset_terminal_modes();
        default_hook(info);
      }
      // Worker panics become failed jobs; printing them would corrupt the
      // screen, so they only go to the log.
    }));
  });
}

/// Forward terminal input to the UI loop from a dedicated thread.
///
/// While `enabled` is false (an external editor owns the terminal) the thread
/// stops reading. Each event carries the `generation` current when it was
/// read, so the UI can drop input that arrived before the editor returned.
pub fn spawn_input_thread(
  tx: mpsc::UnboundedSender<AsyncEvent>,
  enabled: Arc<AtomicBool>,
  generation: Arc<AtomicU64>,
) {
  thread::spawn(move || {
    loop {
      if !enabled.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(25));
        continue;
      }
      match crossterm_event::poll(Duration::from_millis(50)) {
        Ok(true) => {
          if !enabled.load(Ordering::SeqCst) {
            continue;
          }
          let Ok(input) = crossterm_event::read() else {
            break;
          };
          if !enabled.load(Ordering::SeqCst) {
            continue;
          }
          let generation = generation.load(Ordering::SeqCst);
          if tx
            .send(AsyncEvent::Input {
              event: input,
              generation,
            })
            .is_err()
          {
            break;
          }
        }
        Ok(false) => {}
        Err(_) => break,
      }
    }
  });
}

/// Drop input queued while an external program owned the terminal.
pub fn discard_pending_input() {
  for _ in 0..256 {
    match crossterm_event::poll(Duration::from_millis(0)) {
      Ok(true) => {
        if crossterm_event::read().is_err() {
          break;
        }
      }
      Ok(false) | Err(_) => break,
    }
  }
}
