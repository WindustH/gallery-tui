# gallery-tui

`gallery-tui` is a terminal image gallery built with Ratatui. It scans an image
folder and displays the images as navigable cards in a TUI.

![gallery-tui demo](https://github.com/WindustH/gallery-tui/releases/download/v0.1.0/demo.png)

## Features

- Scrollable image-card gallery with keyboard and mouse focus navigation.
- Switchable grid, list, and masonry layouts through `:layout`.
- Detail view with large image preview and filesystem/EXIF metadata.
- Context-aware keymaps inspired by Yazi, including which-key style hints.
- Rename, select, batch path export, refresh, cache clearing, and flexible sort commands.
- Terminal graphics support with Kitty, Sixel, iTerm2, Chafa symbols, and ASCII fallback.
- On-demand async rendering with zstd-compressed LRU render cache.

## Usage

```sh
gallery-tui /path/to/images
```

Batch path output from `c p` is written to stdout after the UI exits, so it can
be piped:

```sh
gallery-tui ~/Pictures | other-tool
```

## Documentation

Full documentation is organized under [doc/index.md](doc/index.md).
