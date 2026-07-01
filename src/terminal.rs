use std::{
  io::{self, Stderr, Write},
  thread,
  time::Duration,
};

use anyhow::Result;
use crossterm::{
  cursor::{MoveTo, RestorePosition, SavePosition},
  event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Frame, Terminal, prelude::CrosstermBackend};

use crate::{
  capability::RenderMode,
  event::{ProtocolOverlay, ProtocolPlacement},
};

pub struct Tui {
  terminal: Terminal<CrosstermBackend<Stderr>>,
  protocol_state: Vec<ProtocolOverlayState>,
  protocol_reset: Option<String>,
  suspended: bool,
  restored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProtocolOverlayState {
  area: ratatui::layout::Rect,
  mode: RenderMode,
  placement: Option<ProtocolPlacement>,
  fingerprint: u64,
  erase: Option<String>,
}

impl Tui {
  pub fn new(protocol_reset: Option<String>) -> Result<Self> {
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(
      stderr,
      EnterAlternateScreen,
      EnableMouseCapture,
      EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;
    reset_protocol_images(terminal.backend_mut(), protocol_reset.as_deref())?;
    Ok(Self {
      terminal,
      protocol_state: Vec::new(),
      protocol_reset,
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
            placement: overlay.placement.clone(),
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
      execute!(backend, SavePosition)?;
      move_to_protocol_area(backend, overlay.area, is_tmux_passthrough(&overlay.data))?;
      backend.write_all(overlay.data.as_bytes())?;
      if let Some(ProtocolPlacement::KittyUnicode { image_id }) = &overlay.placement {
        write_kitty_unicode_placeholders(backend, overlay.area, *image_id)?;
      }
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
    let backend = self.terminal.backend_mut();
    erase_protocol_state(backend, &old_state)?;
    reset_protocol_images(backend, self.protocol_reset.as_deref())?;
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
    let old_state = std::mem::take(&mut self.protocol_state);
    let backend = self.terminal.backend_mut();
    erase_protocol_state(backend, &old_state)?;
    reset_protocol_images(backend, self.protocol_reset.as_deref())?;
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
    reset_protocol_images(self.terminal.backend_mut(), self.protocol_reset.as_deref())?;
    self.suspended = false;
    Ok(())
  }
}

fn move_to_protocol_area(
  backend: &mut CrosstermBackend<Stderr>,
  area: ratatui::layout::Rect,
  tmux_passthrough: bool,
) -> Result<()> {
  execute!(backend, MoveTo(area.x, area.y))?;
  if tmux_passthrough {
    execute!(backend, MoveTo(area.x, area.y), MoveTo(area.x, area.y))?;
    backend.flush()?;
    thread::sleep(Duration::from_millis(1));
  }
  Ok(())
}

fn is_tmux_passthrough(data: &str) -> bool {
  data.starts_with("\x1bPtmux;")
}

fn reset_protocol_images(
  backend: &mut CrosstermBackend<Stderr>,
  sequence: Option<&str>,
) -> Result<()> {
  let Some(sequence) = sequence else {
    return Ok(());
  };
  execute!(backend, SavePosition)?;
  backend.write_all(sequence.as_bytes())?;
  execute!(backend, RestorePosition)?;
  backend.flush()?;
  Ok(())
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

fn write_kitty_unicode_placeholders(
  backend: &mut CrosstermBackend<Stderr>,
  area: ratatui::layout::Rect,
  image_id: u32,
) -> Result<()> {
  if area.width == 0 || area.height == 0 {
    return Ok(());
  }

  let red = (image_id >> 16) & 0xff;
  let green = (image_id >> 8) & 0xff;
  let blue = image_id & 0xff;
  write!(backend, "\x1b[38;2;{red};{green};{blue}m")?;

  for y in 0..area.height {
    execute!(backend, MoveTo(area.x, area.y.saturating_add(y)))?;
    let row = kitty_placeholder_diacritic(y);
    for x in 0..area.width {
      write!(
        backend,
        "{}{}{}",
        KITTY_PLACEHOLDER,
        row,
        kitty_placeholder_diacritic(x)
      )?;
    }
  }

  Ok(())
}

const KITTY_PLACEHOLDER: char = '\u{10EEEE}';
const KITTY_DIACRITICS: &[char] = &[
  '\u{0305}',
  '\u{030D}',
  '\u{030E}',
  '\u{0310}',
  '\u{0312}',
  '\u{033D}',
  '\u{033E}',
  '\u{033F}',
  '\u{0346}',
  '\u{034A}',
  '\u{034B}',
  '\u{034C}',
  '\u{0350}',
  '\u{0351}',
  '\u{0352}',
  '\u{0357}',
  '\u{035B}',
  '\u{0363}',
  '\u{0364}',
  '\u{0365}',
  '\u{0366}',
  '\u{0367}',
  '\u{0368}',
  '\u{0369}',
  '\u{036A}',
  '\u{036B}',
  '\u{036C}',
  '\u{036D}',
  '\u{036E}',
  '\u{036F}',
  '\u{0483}',
  '\u{0484}',
  '\u{0485}',
  '\u{0486}',
  '\u{0487}',
  '\u{0592}',
  '\u{0593}',
  '\u{0594}',
  '\u{0595}',
  '\u{0597}',
  '\u{0598}',
  '\u{0599}',
  '\u{059C}',
  '\u{059D}',
  '\u{059E}',
  '\u{059F}',
  '\u{05A0}',
  '\u{05A1}',
  '\u{05A8}',
  '\u{05A9}',
  '\u{05AB}',
  '\u{05AC}',
  '\u{05AF}',
  '\u{05C4}',
  '\u{0610}',
  '\u{0611}',
  '\u{0612}',
  '\u{0613}',
  '\u{0614}',
  '\u{0615}',
  '\u{0616}',
  '\u{0617}',
  '\u{0657}',
  '\u{0658}',
  '\u{0659}',
  '\u{065A}',
  '\u{065B}',
  '\u{065D}',
  '\u{065E}',
  '\u{06D6}',
  '\u{06D7}',
  '\u{06D8}',
  '\u{06D9}',
  '\u{06DA}',
  '\u{06DB}',
  '\u{06DC}',
  '\u{06DF}',
  '\u{06E0}',
  '\u{06E1}',
  '\u{06E2}',
  '\u{06E4}',
  '\u{06E7}',
  '\u{06E8}',
  '\u{06EB}',
  '\u{06EC}',
  '\u{0730}',
  '\u{0732}',
  '\u{0733}',
  '\u{0735}',
  '\u{0736}',
  '\u{073A}',
  '\u{073D}',
  '\u{073F}',
  '\u{0740}',
  '\u{0741}',
  '\u{0743}',
  '\u{0745}',
  '\u{0747}',
  '\u{0749}',
  '\u{074A}',
  '\u{07EB}',
  '\u{07EC}',
  '\u{07ED}',
  '\u{07EE}',
  '\u{07EF}',
  '\u{07F0}',
  '\u{07F1}',
  '\u{07F3}',
  '\u{0816}',
  '\u{0817}',
  '\u{0818}',
  '\u{0819}',
  '\u{081B}',
  '\u{081C}',
  '\u{081D}',
  '\u{081E}',
  '\u{081F}',
  '\u{0820}',
  '\u{0821}',
  '\u{0822}',
  '\u{0823}',
  '\u{0825}',
  '\u{0826}',
  '\u{0827}',
  '\u{0829}',
  '\u{082A}',
  '\u{082B}',
  '\u{082C}',
  '\u{082D}',
  '\u{0951}',
  '\u{0953}',
  '\u{0954}',
  '\u{0F82}',
  '\u{0F83}',
  '\u{0F86}',
  '\u{0F87}',
  '\u{135D}',
  '\u{135E}',
  '\u{135F}',
  '\u{17DD}',
  '\u{193A}',
  '\u{1A17}',
  '\u{1A75}',
  '\u{1A76}',
  '\u{1A77}',
  '\u{1A78}',
  '\u{1A79}',
  '\u{1A7A}',
  '\u{1A7B}',
  '\u{1A7C}',
  '\u{1B6B}',
  '\u{1B6D}',
  '\u{1B6E}',
  '\u{1B6F}',
  '\u{1B70}',
  '\u{1B71}',
  '\u{1B72}',
  '\u{1B73}',
  '\u{1CD0}',
  '\u{1CD1}',
  '\u{1CD2}',
  '\u{1CDA}',
  '\u{1CDB}',
  '\u{1CE0}',
  '\u{1DC0}',
  '\u{1DC1}',
  '\u{1DC3}',
  '\u{1DC4}',
  '\u{1DC5}',
  '\u{1DC6}',
  '\u{1DC7}',
  '\u{1DC8}',
  '\u{1DC9}',
  '\u{1DCB}',
  '\u{1DCC}',
  '\u{1DD1}',
  '\u{1DD2}',
  '\u{1DD3}',
  '\u{1DD4}',
  '\u{1DD5}',
  '\u{1DD6}',
  '\u{1DD7}',
  '\u{1DD8}',
  '\u{1DD9}',
  '\u{1DDA}',
  '\u{1DDB}',
  '\u{1DDC}',
  '\u{1DDD}',
  '\u{1DDE}',
  '\u{1DDF}',
  '\u{1DE0}',
  '\u{1DE1}',
  '\u{1DE2}',
  '\u{1DE3}',
  '\u{1DE4}',
  '\u{1DE5}',
  '\u{1DE6}',
  '\u{1DFE}',
  '\u{20D0}',
  '\u{20D1}',
  '\u{20D4}',
  '\u{20D5}',
  '\u{20D6}',
  '\u{20D7}',
  '\u{20DB}',
  '\u{20DC}',
  '\u{20E1}',
  '\u{20E7}',
  '\u{20E9}',
  '\u{20F0}',
  '\u{2CEF}',
  '\u{2CF0}',
  '\u{2CF1}',
  '\u{2DE0}',
  '\u{2DE1}',
  '\u{2DE2}',
  '\u{2DE3}',
  '\u{2DE4}',
  '\u{2DE5}',
  '\u{2DE6}',
  '\u{2DE7}',
  '\u{2DE8}',
  '\u{2DE9}',
  '\u{2DEA}',
  '\u{2DEB}',
  '\u{2DEC}',
  '\u{2DED}',
  '\u{2DEE}',
  '\u{2DEF}',
  '\u{2DF0}',
  '\u{2DF1}',
  '\u{2DF2}',
  '\u{2DF3}',
  '\u{2DF4}',
  '\u{2DF5}',
  '\u{2DF6}',
  '\u{2DF7}',
  '\u{2DF8}',
  '\u{2DF9}',
  '\u{2DFA}',
  '\u{2DFB}',
  '\u{2DFC}',
  '\u{2DFD}',
  '\u{2DFE}',
  '\u{2DFF}',
  '\u{A66F}',
  '\u{A67C}',
  '\u{A67D}',
  '\u{A6F0}',
  '\u{A6F1}',
  '\u{A8E0}',
  '\u{A8E1}',
  '\u{A8E2}',
  '\u{A8E3}',
  '\u{A8E4}',
  '\u{A8E5}',
  '\u{A8E6}',
  '\u{A8E7}',
  '\u{A8E8}',
  '\u{A8E9}',
  '\u{A8EA}',
  '\u{A8EB}',
  '\u{A8EC}',
  '\u{A8ED}',
  '\u{A8EE}',
  '\u{A8EF}',
  '\u{A8F0}',
  '\u{A8F1}',
  '\u{AAB0}',
  '\u{AAB2}',
  '\u{AAB3}',
  '\u{AAB7}',
  '\u{AAB8}',
  '\u{AABE}',
  '\u{AABF}',
  '\u{AAC1}',
  '\u{FE20}',
  '\u{FE21}',
  '\u{FE22}',
  '\u{FE23}',
  '\u{FE24}',
  '\u{FE25}',
  '\u{FE26}',
  '\u{10A0F}',
  '\u{10A38}',
  '\u{1D185}',
  '\u{1D186}',
  '\u{1D187}',
  '\u{1D188}',
  '\u{1D189}',
  '\u{1D1AA}',
  '\u{1D1AB}',
  '\u{1D1AC}',
  '\u{1D1AD}',
  '\u{1D242}',
  '\u{1D243}',
  '\u{1D244}',
];

fn kitty_placeholder_diacritic(index: u16) -> char {
  KITTY_DIACRITICS
    .get(index as usize)
    .copied()
    .unwrap_or(KITTY_DIACRITICS[0])
}

impl Drop for Tui {
  fn drop(&mut self) {
    let _ = self.restore();
  }
}
