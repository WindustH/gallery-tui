use super::*;

#[test]
fn detects_yazi_style_probe_tokens() {
  let response = "\x1b_Gi=31;OK\x1b\\\x1b[?1;2;4c";
  assert!(supports_kitty_graphics(response));
  assert!(supports_sixel(response));
}

#[test]
fn zellij_sixel_is_off_by_default() {
  let capability = zellij_capability(false);
  assert_eq!(
    capability.preferred_render_modes("off"),
    vec![RenderMode::Symbols, RenderMode::Ascii]
  );
}

#[test]
fn zellij_auto_requires_probe_sixel() {
  let unsupported = zellij_capability(false);
  assert_eq!(
    unsupported.preferred_render_modes("auto"),
    vec![RenderMode::Symbols, RenderMode::Ascii]
  );

  let supported = zellij_capability(true);
  assert_eq!(
    supported.preferred_render_modes("auto"),
    vec![RenderMode::Sixel, RenderMode::Symbols, RenderMode::Ascii]
  );
}

#[test]
fn zellij_on_forces_sixel_before_symbol_fallback() {
  let capability = zellij_capability(false);
  assert_eq!(
    capability.preferred_render_modes("on"),
    vec![RenderMode::Sixel, RenderMode::Symbols, RenderMode::Ascii]
  );
}

fn zellij_capability(probe_sixel: bool) -> TerminalCapability {
  TerminalCapability {
    term: None,
    term_program: None,
    colorterm: None,
    multiplexer: Some("zellij".to_string()),
    brand: None,
    probe: TerminalProbe {
      attempted: true,
      response_bytes: 0,
      kitty_graphics: false,
      sixel: probe_sixel,
      cell_pixels: None,
      brand: None,
      error: None,
    },
    pixel_protocols: vec![
      PixelProtocol::Kitty,
      PixelProtocol::Sixel,
      PixelProtocol::Iterm2,
    ],
    color_level: ColorLevel::TrueColor,
    cell_pixels: None,
  }
}

#[test]
fn parses_cell_pixel_size_response() {
  assert_eq!(csi_16t("\x1b[6;18;9t\x1b[?1;2c"), Some((9, 18)));
}
