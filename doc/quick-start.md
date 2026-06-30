# Quick Start

Run `gallery-tui` with an image directory:

```sh
gallery-tui /path/to/images
```

The TUI writes interface output to stderr. Batch path output from `c p` is
written to stdout after the UI exits, so it can be piped:

```sh
gallery-tui ~/Pictures | other-tool
```

Default configuration files are created on first run:

- `~/.config/gallery-tui/config.toml`
- `~/.config/gallery-tui/keymap.toml`
- `~/.config/gallery-tui/theme.toml`

Rendered image cache and logs are stored under:

- `~/.cache/gallery-tui/`

Basic workflow:

1. Move focus with `h/j/k/l`, arrow keys, mouse wheel, or mouse click.
2. Press `enter` to open detail view.
3. Use `h/l` in detail view to switch between image and metadata pages.
4. Press `q` in detail view to return to the browser.
5. Press `q` in browser view to quit.
