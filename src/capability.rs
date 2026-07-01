use std::{
  env,
  process::{Command, Stdio},
  time::Duration,
};

#[cfg(unix)]
use std::{
  fs::{File, OpenOptions},
  io::{ErrorKind, Read, Write},
  os::fd::{AsRawFd, RawFd},
  time::Instant,
};

use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelProtocol {
  Kitty,
  Sixel,
  Iterm2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderMode {
  Kitty,
  Sixel,
  Iterm2,
  Symbols,
  Ascii,
}

impl RenderMode {
  pub fn chafa_format(self) -> &'static str {
    match self {
      Self::Kitty => "kitty",
      Self::Sixel => "sixels",
      Self::Iterm2 => "iterm",
      Self::Symbols | Self::Ascii => "symbols",
    }
  }

  pub fn is_protocol(self) -> bool {
    matches!(self, Self::Kitty | Self::Sixel | Self::Iterm2)
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Kitty => "kitty",
      Self::Sixel => "sixel",
      Self::Iterm2 => "iterm",
      Self::Symbols => "symbols",
      Self::Ascii => "ascii",
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCapability {
  pub term: Option<String>,
  pub term_program: Option<String>,
  pub colorterm: Option<String>,
  pub multiplexer: Option<String>,
  pub brand: Option<String>,
  pub probe: TerminalProbe,
  pub pixel_protocols: Vec<PixelProtocol>,
  pub color_level: ColorLevel,
  pub cell_pixels: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalProbe {
  pub attempted: bool,
  pub response_bytes: usize,
  pub kitty_graphics: bool,
  pub sixel: bool,
  pub cell_pixels: Option<(u16, u16)>,
  pub brand: Option<String>,
  pub error: Option<String>,
}

impl TerminalProbe {
  fn skipped(message: impl Into<String>) -> Self {
    Self {
      attempted: false,
      response_bytes: 0,
      kitty_graphics: false,
      sixel: false,
      cell_pixels: None,
      brand: None,
      error: Some(message.into()),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorLevel {
  TrueColor,
  Ansi256,
  Ansi16,
  Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProbeRequests {
  kitty_graphics: bool,
  terminal_version: bool,
  cell_pixels: bool,
  da1: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalBrand {
  Kitty,
  Konsole,
  Iterm2,
  WezTerm,
  Foot,
  Ghostty,
  Microsoft,
  Warp,
  Rio,
  BlackBox,
  VSCode,
  Tabby,
  Hyper,
  Mintty,
  Tmux,
  VTerm,
  Apple,
  Urxvt,
  Bobcat,
}

impl TerminalBrand {
  fn label(self) -> &'static str {
    match self {
      Self::Kitty => "kitty",
      Self::Konsole => "konsole",
      Self::Iterm2 => "iterm2",
      Self::WezTerm => "wezterm",
      Self::Foot => "foot",
      Self::Ghostty => "ghostty",
      Self::Microsoft => "windows-terminal",
      Self::Warp => "warp",
      Self::Rio => "rio",
      Self::BlackBox => "blackbox",
      Self::VSCode => "vscode",
      Self::Tabby => "tabby",
      Self::Hyper => "hyper",
      Self::Mintty => "mintty",
      Self::Tmux => "tmux",
      Self::VTerm => "vterm",
      Self::Apple => "apple-terminal",
      Self::Urxvt => "urxvt",
      Self::Bobcat => "bobcat",
    }
  }
}

impl TerminalCapability {
  pub fn preferred_render_modes(&self, zellij_sixel: &str) -> Vec<RenderMode> {
    let mut modes = Vec::new();
    if self.multiplexer.as_deref() == Some("zellij") {
      let allow_sixel = match zellij_sixel.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" => self.pixel_protocols.contains(&PixelProtocol::Sixel),
        "auto" => self.probe.sixel && self.pixel_protocols.contains(&PixelProtocol::Sixel),
        _ => false,
      };
      if allow_sixel {
        modes.push(RenderMode::Sixel);
      }
      modes.push(RenderMode::Symbols);
      modes.push(RenderMode::Ascii);
      return modes;
    }

    let protocol_order = &[
      PixelProtocol::Kitty,
      PixelProtocol::Sixel,
      PixelProtocol::Iterm2,
    ];

    for protocol in protocol_order {
      if self.pixel_protocols.contains(protocol) {
        modes.push(match protocol {
          PixelProtocol::Kitty => RenderMode::Kitty,
          PixelProtocol::Sixel => RenderMode::Sixel,
          PixelProtocol::Iterm2 => RenderMode::Iterm2,
        });
      }
    }
    modes.push(RenderMode::Symbols);
    modes.push(RenderMode::Ascii);
    modes
  }

  pub fn colors_arg(&self) -> &'static str {
    match self.color_level {
      ColorLevel::TrueColor => "--colors=full",
      ColorLevel::Ansi256 => "--colors=256",
      ColorLevel::Ansi16 => "--colors=16",
      ColorLevel::Mono => "--colors=none",
    }
  }

  pub fn symbols_arg(&self) -> &'static str {
    match self.color_level {
      ColorLevel::Mono => "--symbols=ascii",
      _ => "--symbols=block",
    }
  }

  pub fn passthrough(&self) -> Option<&'static str> {
    match self.multiplexer.as_deref() {
      Some("tmux") => Some("tmux"),
      Some("screen") => Some("screen"),
      _ => None,
    }
  }
}

pub fn detect() -> TerminalCapability {
  let term = env::var("TERM").ok();
  let term_program = env::var("TERM_PROGRAM").ok();
  let colorterm = env::var("COLORTERM").ok();
  let multiplexer = detect_multiplexer();
  if multiplexer.as_deref() == Some("tmux") {
    enable_tmux_passthrough();
  }
  let env_brand = detect_brand_from_env(term.as_deref(), term_program.as_deref());
  let needs_identity_probe = env_brand.is_none() && multiplexer.as_deref() != Some("zellij");
  let requests = ProbeRequests {
    kitty_graphics: needs_identity_probe,
    terminal_version: needs_identity_probe,
    cell_pixels: true,
    da1: env_brand.is_none() || multiplexer.as_deref() == Some("zellij"),
  };
  let mut active_probe = probe_terminal(multiplexer.as_deref(), requests);
  if active_probe.summary.cell_pixels.is_none() {
    active_probe.summary.cell_pixels = detect_window_cell_pixels();
  }
  let probed_brand = active_probe.brand;
  let brand = probed_brand.or(env_brand);

  let term_lower = term.as_deref().unwrap_or_default().to_ascii_lowercase();
  let program_lower = term_program
    .as_deref()
    .unwrap_or_default()
    .to_ascii_lowercase();
  let colorterm_lower = colorterm
    .as_deref()
    .unwrap_or_default()
    .to_ascii_lowercase();

  let mut pixel_protocols = Vec::new();
  if active_probe.summary.kitty_graphics {
    push_protocol(&mut pixel_protocols, PixelProtocol::Kitty);
  }
  if active_probe.summary.sixel {
    push_protocol(&mut pixel_protocols, PixelProtocol::Sixel);
  }
  if let Some(brand) = brand {
    for protocol in protocols_for_brand(brand) {
      push_protocol(&mut pixel_protocols, *protocol);
    }
  }
  add_environment_protocol_hints(&mut pixel_protocols, &term_lower, &program_lower);

  if multiplexer.as_deref() == Some("zellij") {
    pixel_protocols.retain(|protocol| *protocol == PixelProtocol::Sixel);
  }

  let color_level = if colorterm_lower.contains("truecolor")
    || colorterm_lower.contains("24bit")
    || program_lower.contains("wezterm")
    || term_lower.contains("kitty")
  {
    ColorLevel::TrueColor
  } else if term_lower.contains("256color") {
    ColorLevel::Ansi256
  } else if term_lower == "dumb" || term_lower.is_empty() {
    ColorLevel::Mono
  } else {
    ColorLevel::Ansi16
  };

  let cell_pixels = active_probe.summary.cell_pixels;
  TerminalCapability {
    term,
    term_program,
    colorterm,
    multiplexer,
    brand: brand.map(|brand| brand.label().to_string()),
    probe: active_probe.summary,
    pixel_protocols,
    color_level,
    cell_pixels,
  }
}

fn enable_tmux_passthrough() {
  match Command::new("tmux")
    .args(["set", "-p", "allow-passthrough", "on"])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .status()
  {
    Ok(status) if status.success() => {}
    Ok(status) => warn!(?status, "failed to enable tmux passthrough"),
    Err(error) => warn!(%error, "failed to run tmux passthrough setup"),
  }
}

struct ActiveProbe {
  summary: TerminalProbe,
  brand: Option<TerminalBrand>,
}

fn probe_terminal(multiplexer: Option<&str>, requests: ProbeRequests) -> ActiveProbe {
  match query_terminal(multiplexer, requests) {
    Ok(response) => {
      let brand = detect_brand_from_response(&response);
      let summary = TerminalProbe {
        attempted: true,
        response_bytes: response.len(),
        kitty_graphics: supports_kitty_graphics(&response),
        sixel: supports_sixel(&response),
        cell_pixels: csi_16t(&response),
        brand: brand.map(|brand| brand.label().to_string()),
        error: None,
      };
      debug!(?summary, "terminal probe completed");
      ActiveProbe { summary, brand }
    }
    Err(error) => {
      warn!(error, "terminal probe failed");
      ActiveProbe {
        summary: TerminalProbe::skipped(error),
        brand: None,
      }
    }
  }
}

#[cfg(unix)]
fn query_terminal(multiplexer: Option<&str>, requests: ProbeRequests) -> Result<String, String> {
  const KITTY_GRAPHICS_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
  const REQUEST_XT_VERSION: &str = "\x1b[>q";
  const REQUEST_CELL_PIXEL_SIZE: &str = "\x1b[16t";
  const REQUEST_DA1: &str = "\x1b[0c";
  const SAVE_CURSOR: &str = "\x1b[s";
  const RESTORE_CURSOR: &str = "\x1b[u";
  const PROBE_TIMEOUT: Duration = Duration::from_millis(800);
  const PROBE_QUIET_TIMEOUT: Duration = Duration::from_millis(120);

  let mut tty = OpenOptions::new()
    .read(true)
    .write(true)
    .open("/dev/tty")
    .map_err(|err| format!("failed to open /dev/tty: {err}"))?;
  let _raw = RawModeGuard::enable(tty.as_raw_fd())
    .map_err(|err| format!("failed to enter raw mode for probe: {err}"))?;

  let mut request = String::new();
  request.push_str(SAVE_CURSOR);
  if requests.kitty_graphics {
    request.push_str(&wrap_probe_sequence(KITTY_GRAPHICS_QUERY, multiplexer));
  }
  if requests.terminal_version {
    request.push_str(&wrap_probe_sequence(REQUEST_XT_VERSION, multiplexer));
  }
  if requests.cell_pixels {
    request.push_str(REQUEST_CELL_PIXEL_SIZE);
  }
  if requests.da1 {
    request.push_str(&wrap_probe_sequence(REQUEST_DA1, multiplexer));
  }
  request.push_str(RESTORE_CURSOR);
  tty
    .write_all(request.as_bytes())
    .and_then(|_| tty.flush())
    .map_err(|err| format!("failed to write terminal probe: {err}"))?;

  let bytes = read_probe_responses(&mut tty, requests, PROBE_TIMEOUT, PROBE_QUIET_TIMEOUT)
    .map_err(|err| format!("failed to read terminal probe response: {err}"))?;
  flush_terminal_input(tty.as_raw_fd());
  Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(not(unix))]
fn query_terminal(_multiplexer: Option<&str>, _requests: ProbeRequests) -> Result<String, String> {
  Err("active terminal probing is only implemented on Unix".to_string())
}

fn wrap_probe_sequence(sequence: &str, multiplexer: Option<&str>) -> String {
  if multiplexer != Some("tmux") {
    return sequence.to_string();
  }

  let escaped = sequence
    .trim_start_matches('\x1b')
    .replace('\x1b', "\x1b\x1b");
  format!("\x1bPtmux;\x1b\x1b{escaped}\x1b\\")
}

#[cfg(unix)]
fn read_probe_responses(
  tty: &mut File,
  requests: ProbeRequests,
  timeout: Duration,
  quiet_timeout: Duration,
) -> std::io::Result<Vec<u8>> {
  let started = Instant::now();
  let mut last_byte_at = None;
  let mut buf = Vec::with_capacity(256);
  while started.elapsed() < timeout {
    if probe_responses_complete(&buf, requests) {
      break;
    }
    let probe_went_quiet =
      last_byte_at.is_some_and(|last: Instant| last.elapsed() >= quiet_timeout);
    if !buf.is_empty() && probe_went_quiet {
      break;
    }

    let remaining = timeout.saturating_sub(started.elapsed());
    let poll_timeout = remaining.min(Duration::from_millis(30));
    if !poll_readable(tty.as_raw_fd(), poll_timeout)? {
      continue;
    }

    let mut byte = [0_u8; 1];
    match tty.read(&mut byte) {
      Ok(0) => break,
      Ok(_) => {
        buf.push(byte[0]);
        last_byte_at = Some(Instant::now());
      }
      Err(err) if err.kind() == ErrorKind::Interrupted => continue,
      Err(err) => return Err(err),
    }
  }
  Ok(buf)
}

#[cfg(unix)]
fn flush_terminal_input(fd: RawFd) {
  if unsafe { libc::tcflush(fd, libc::TCIFLUSH) } == -1 {
    warn!(
      error = %std::io::Error::last_os_error(),
      "failed to flush terminal input after probe"
    );
  }
}

#[cfg(unix)]
fn poll_readable(fd: RawFd, timeout: Duration) -> std::io::Result<bool> {
  let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
  let mut pollfd = libc::pollfd {
    fd,
    events: libc::POLLIN,
    revents: 0,
  };
  loop {
    match unsafe { libc::poll(&mut pollfd, 1, timeout_ms) } {
      -1 => {
        let error = std::io::Error::last_os_error();
        if error.kind() == ErrorKind::Interrupted {
          continue;
        }
        return Err(error);
      }
      0 => return Ok(false),
      _ => return Ok((pollfd.revents & libc::POLLIN) != 0),
    }
  }
}

#[cfg(unix)]
struct RawModeGuard {
  fd: RawFd,
  original: libc::termios,
}

#[cfg(unix)]
impl RawModeGuard {
  fn enable(fd: RawFd) -> std::io::Result<Self> {
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } == -1 {
      return Err(std::io::Error::last_os_error());
    }
    let original = unsafe { original.assume_init() };
    let mut raw = original;
    unsafe { libc::cfmakeraw(&mut raw) };
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } == -1 {
      return Err(std::io::Error::last_os_error());
    }
    Ok(Self { fd, original })
  }
}

#[cfg(unix)]
impl Drop for RawModeGuard {
  fn drop(&mut self) {
    if unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.original) } == -1 {
      warn!(
        error = %std::io::Error::last_os_error(),
        "failed to restore terminal mode after probe"
      );
    }
  }
}

fn is_da1_response_end(byte: u8, buf: &[u8]) -> bool {
  byte == b'c'
    && buf.contains(&0x1b)
    && buf
      .rsplitn(2, |candidate| *candidate == 0x1b)
      .next()
      .is_some_and(|tail| tail.starts_with(b"[?"))
}

fn probe_responses_complete(buf: &[u8], requests: ProbeRequests) -> bool {
  let response = String::from_utf8_lossy(buf);
  (!requests.kitty_graphics || has_kitty_graphics_response(buf))
    && (!requests.terminal_version || has_xt_version_response(buf))
    && (!requests.cell_pixels || csi_16t(&response).is_some())
    && (!requests.da1 || has_da1_response(buf))
}

fn has_kitty_graphics_response(buf: &[u8]) -> bool {
  terminated_after(buf, b"\x1b_Gi=31", b"\x1b\\")
}

fn has_xt_version_response(buf: &[u8]) -> bool {
  terminated_after(buf, b"\x1bP>|", b"\x1b\\")
}

fn has_da1_response(buf: &[u8]) -> bool {
  buf
    .iter()
    .enumerate()
    .any(|(index, byte)| is_da1_response_end(*byte, &buf[..=index]))
}

fn terminated_after(buf: &[u8], start: &[u8], terminator: &[u8]) -> bool {
  let Some(start_index) = find_subslice(buf, start) else {
    return false;
  };
  find_subslice(&buf[start_index + start.len()..], terminator).is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  if needle.is_empty() {
    return Some(0);
  }
  haystack
    .windows(needle.len())
    .position(|candidate| candidate == needle)
}

fn supports_kitty_graphics(response: &str) -> bool {
  response.contains("\x1b_Gi=31;OK")
}

fn supports_sixel(response: &str) -> bool {
  ["?4;", "?4c", ";4;", ";4c"]
    .iter()
    .any(|needle| response.contains(needle))
}

fn csi_16t(response: &str) -> Option<(u16, u16)> {
  let bytes = response.split_once("\x1b[6;")?.1.as_bytes();

  let height: Vec<_> = bytes
    .iter()
    .copied()
    .take_while(|byte| byte.is_ascii_digit())
    .collect();
  bytes.get(height.len()).filter(|byte| **byte == b';')?;

  let width_start = height.len() + 1;
  let width: Vec<_> = bytes[width_start..]
    .iter()
    .copied()
    .take_while(|byte| byte.is_ascii_digit())
    .collect();
  bytes
    .get(width_start + width.len())
    .filter(|byte| **byte == b't')?;

  let height = String::from_utf8(height).ok()?.parse().ok()?;
  let width = String::from_utf8(width).ok()?.parse().ok()?;
  Some((width, height))
}

#[cfg(unix)]
fn detect_window_cell_pixels() -> Option<(u16, u16)> {
  let tty = OpenOptions::new()
    .read(true)
    .write(true)
    .open("/dev/tty")
    .ok()?;
  let mut winsize = std::mem::MaybeUninit::<libc::winsize>::zeroed();
  if unsafe { libc::ioctl(tty.as_raw_fd(), libc::TIOCGWINSZ, winsize.as_mut_ptr()) } == -1 {
    return None;
  }

  let winsize = unsafe { winsize.assume_init() };
  if winsize.ws_col == 0 || winsize.ws_row == 0 || winsize.ws_xpixel == 0 || winsize.ws_ypixel == 0
  {
    return None;
  }

  Some((
    (winsize.ws_xpixel / winsize.ws_col).max(1),
    (winsize.ws_ypixel / winsize.ws_row).max(1),
  ))
}

#[cfg(not(unix))]
fn detect_window_cell_pixels() -> Option<(u16, u16)> {
  None
}

fn detect_brand_from_response(response: &str) -> Option<TerminalBrand> {
  [
    ("kitty", TerminalBrand::Kitty),
    ("Konsole", TerminalBrand::Konsole),
    ("iTerm2", TerminalBrand::Iterm2),
    ("WezTerm", TerminalBrand::WezTerm),
    ("foot", TerminalBrand::Foot),
    ("ghostty", TerminalBrand::Ghostty),
    ("Warp", TerminalBrand::Warp),
    ("Rio ", TerminalBrand::Rio),
    ("tmux ", TerminalBrand::Tmux),
    ("libvterm", TerminalBrand::VTerm),
    ("Bobcat", TerminalBrand::Bobcat),
  ]
  .into_iter()
  .find(|(needle, _)| response.contains(needle))
  .map(|(_, brand)| brand)
}

fn detect_brand_from_env(term: Option<&str>, term_program: Option<&str>) -> Option<TerminalBrand> {
  let term = term.unwrap_or_default();
  let term_lower = term.to_ascii_lowercase();
  let program = term_program.unwrap_or_default();
  let program_lower = program.to_ascii_lowercase();

  match term_lower.as_str() {
    "xterm-kitty" => return Some(TerminalBrand::Kitty),
    "foot" | "foot-extra" => return Some(TerminalBrand::Foot),
    "xterm-ghostty" => return Some(TerminalBrand::Ghostty),
    "rio" => return Some(TerminalBrand::Rio),
    "rxvt-unicode-256color" => return Some(TerminalBrand::Urxvt),
    _ => {}
  }

  match program {
    "iTerm.app" => return Some(TerminalBrand::Iterm2),
    "WezTerm" => return Some(TerminalBrand::WezTerm),
    "WarpTerminal" => return Some(TerminalBrand::Warp),
    "Apple_Terminal" => return Some(TerminalBrand::Apple),
    _ => {}
  }
  match program_lower.as_str() {
    "ghostty" => return Some(TerminalBrand::Ghostty),
    "rio" => return Some(TerminalBrand::Rio),
    "blackbox" => return Some(TerminalBrand::BlackBox),
    "vscode" => return Some(TerminalBrand::VSCode),
    "tabby" => return Some(TerminalBrand::Tabby),
    "hyper" => return Some(TerminalBrand::Hyper),
    "mintty" => return Some(TerminalBrand::Mintty),
    _ => {}
  }

  for (name, brand) in [
    ("KITTY_WINDOW_ID", TerminalBrand::Kitty),
    ("KONSOLE_VERSION", TerminalBrand::Konsole),
    ("ITERM_SESSION_ID", TerminalBrand::Iterm2),
    ("WEZTERM_EXECUTABLE", TerminalBrand::WezTerm),
    ("GHOSTTY_RESOURCES_DIR", TerminalBrand::Ghostty),
    ("WT_SESSION", TerminalBrand::Microsoft),
    ("WT_Session", TerminalBrand::Microsoft),
    ("WARP_HONOR_PS1", TerminalBrand::Warp),
    ("VSCODE_INJECTION", TerminalBrand::VSCode),
    ("TABBY_CONFIG_DIRECTORY", TerminalBrand::Tabby),
  ] {
    if env::var_os(name).is_some() {
      return Some(brand);
    }
  }

  None
}

fn protocols_for_brand(brand: TerminalBrand) -> &'static [PixelProtocol] {
  match brand {
    TerminalBrand::Kitty | TerminalBrand::Konsole | TerminalBrand::Ghostty | TerminalBrand::Rio => {
      &[PixelProtocol::Kitty]
    }
    TerminalBrand::Iterm2
    | TerminalBrand::WezTerm
    | TerminalBrand::VSCode
    | TerminalBrand::Tabby
    | TerminalBrand::Hyper
    | TerminalBrand::Bobcat => &[PixelProtocol::Iterm2, PixelProtocol::Sixel],
    TerminalBrand::Foot | TerminalBrand::Microsoft | TerminalBrand::BlackBox => {
      &[PixelProtocol::Sixel]
    }
    TerminalBrand::Warp => &[PixelProtocol::Iterm2, PixelProtocol::Kitty],
    TerminalBrand::Mintty => &[PixelProtocol::Iterm2],
    TerminalBrand::Tmux | TerminalBrand::VTerm | TerminalBrand::Apple | TerminalBrand::Urxvt => &[],
  }
}

fn add_environment_protocol_hints(
  pixel_protocols: &mut Vec<PixelProtocol>,
  term_lower: &str,
  program_lower: &str,
) {
  if env::var_os("KITTY_WINDOW_ID").is_some()
    || term_lower.contains("kitty")
    || program_lower.contains("ghostty")
    || program_lower.contains("rio")
  {
    push_protocol(pixel_protocols, PixelProtocol::Kitty);
  }
  if program_lower.contains("iterm") {
    push_protocol(pixel_protocols, PixelProtocol::Iterm2);
  }
  if term_lower.contains("sixel")
    || program_lower.contains("foot")
    || program_lower.contains("wezterm")
    || env::var_os("MLTERM").is_some()
  {
    push_protocol(pixel_protocols, PixelProtocol::Sixel);
  }
}

fn push_protocol(protocols: &mut Vec<PixelProtocol>, protocol: PixelProtocol) {
  if !protocols.contains(&protocol) {
    protocols.push(protocol);
  }
}

fn detect_multiplexer() -> Option<String> {
  if env::var_os("ZELLIJ").is_some() || env::var_os("ZELLIJ_SESSION_NAME").is_some() {
    Some("zellij".to_string())
  } else if env::var_os("TMUX").is_some() {
    Some("tmux".to_string())
  } else if env::var_os("STY").is_some() {
    Some("screen".to_string())
  } else {
    None
  }
}
