# Terminal Graphics

When `render.auto_detect` is enabled, gallery-tui probes terminal graphics
support before entering the TUI.

The probe checks:

- Kitty graphics support
- terminal version response
- cell pixel size
- DA1/sixel support

Default render mode order:

1. Kitty
2. Sixel
3. iTerm2
4. Chafa symbols
5. ASCII symbols without color

## Multiple Instances

When kitty graphics are used, each gallery-tui process uses its own image-id
namespace. Exiting or suspending one instance clears the images it knows about
instead of issuing a terminal-wide delete-all command, which avoids interfering
with other gallery-tui instances or other TUI programs running in the same
terminal server.

## Zellij

[Zellij 0.45 and newer](https://zellij.dev/documentation/compatibility.html)
implement the Kitty graphics protocol. `img-tui` sends the standard KGP query
to Zellij and prefers Kitty when both Zellij and its attached host terminal
confirm support. It does not infer support from outer-terminal environment
variables, so disabling `support_kitty_graphics_protocol` in Zellij or using an
unsupported host still falls back safely.

Zellij does not currently implement Kitty Unicode placeholders. gallery-tui
therefore uses regular Kitty placements under Zellij instead of the `U=1`
placeholder path.

`render.zellij_sixel` controls only the secondary Sixel path:

- `off`: never use sixel under zellij
- `auto`: enable sixel only when active probing reports sixel support
- `on`: force the Yazi-style sixel path

The default is `off`. This does not disable Kitty graphics: the effective order
under a capable Zellij is Kitty, Chafa symbols, then ASCII. Older Zellij
versions and sessions without KGP support retain the previous fallback
behavior.
