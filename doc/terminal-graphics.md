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

Zellij is handled conservatively. Rendering many protocol images under zellij
can be unstable depending on the outer terminal.

`render.zellij_sixel` controls behavior:

- `off`: never use sixel under zellij
- `auto`: enable sixel only when active probing reports sixel support
- `on`: force the Yazi-style sixel path

The default is `off`, so zellij uses Chafa symbols/ASCII unless configured
otherwise.
