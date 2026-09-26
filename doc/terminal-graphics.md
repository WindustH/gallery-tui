# Terminal Graphics

## Detection

Before the interface starts, gallery-tui asks the terminal what it supports: the
Kitty graphics protocol, its name and version, the size of a character cell in
pixels, and Sixel support. It combines the answers with environment variables
such as `TERM`, `TERM_PROGRAM`, and `KITTY_WINDOW_ID`, then uses the first
available protocol of Kitty, Sixel, and iTerm2, falling back to Chafa text
output (see [Rendering](rendering.md)).

The cell size also sets how images are fitted into cards, so images keep their
aspect ratio even in text mode.

In kitty, Ghostty, and Rio, Kitty images are drawn with Unicode placeholders,
which lets dialogs and completion lists cover part of an image cleanly.

## Choosing Render Modes

Set `GALLERY_TUI_RENDER_MODES` to override the detected order with a list
separated by commas, colons, or spaces:

| Value | Mode |
| --- | --- |
| `kitty` or `kgp` | Kitty graphics protocol |
| `sixel` | Sixel |
| `iterm`, `iterm2`, or `iip` | iTerm2 inline images |
| `symbols` | Chafa colored block symbols |
| `ascii` | Chafa uncolored ASCII |
| `off`, `none`, `text`, `chafa`, or `fallback` | `symbols` then `ascii` |
| `auto` or empty | Use detection |

```sh
GALLERY_TUI_RENDER_MODES=symbols gallery-tui ~/Pictures
GALLERY_TUI_RENDER_MODES=sixel,symbols gallery-tui ~/Pictures
```

Setting `render.auto_detect = false` in `config.toml` also limits gallery-tui to
the two Chafa modes and keeps the Chafa colors from `render.chafa_args`.

## tmux and GNU screen

Inside tmux, gallery-tui runs `tmux set -p allow-passthrough on` for its pane,
reads the outer terminal's `TERM` and `TERM_PROGRAM` from the tmux environment,
and wraps graphics in tmux passthrough sequences. Inside GNU screen it uses
screen's passthrough.

## Zellij

[Zellij 0.45 and newer](https://zellij.dev/documentation/compatibility.html)
implement the Kitty graphics protocol. gallery-tui sends the standard Kitty
query and uses Kitty only when Zellij answers it, which requires both Zellij's
`support_kitty_graphics_protocol` and a capable host terminal. Environment
variables from the outer terminal are not trusted inside Zellij, so an
unsupported setup falls back to Chafa safely. Zellij does not support Kitty
Unicode placeholders, so regular Kitty placements are used there.

`render.zellij_sixel` controls whether Sixel is tried after Kitty:

- `off` (default): never use Sixel inside Zellij
- `auto` or `on`: use Sixel when the terminal query reports Sixel support

## Several Instances

Each gallery-tui process uses its own Kitty image IDs. When one instance exits
or opens an editor, it deletes only its own images, so other instances and
other programs in the same terminal keep theirs.
