# gallery-tui

`gallery-tui` is a terminal image gallery built with Ratatui. It scans an image
folder and displays the images as navigable cards in a TUI.

![gallery-tui demo](https://media.githubusercontent.com/media/WindustH/gallery-tui-assets/master/demo.png)

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
gallery-tui /path/to/image.png
gallery-tui --browser /path/to/image.png
```

Opening a single image starts in detail view. Pressing `q` exits immediately;
with `--browser`, `q` returns to the folder browser instead.

Batch path output from `c p` is written to stdout after the UI exits, so it can
be piped:

```sh
gallery-tui ~/Pictures | other-tool
```

## Installation

Arch Linux AUR:

```sh
yay -S gallery-tui-bin
```

Alternative AUR packages:

```sh
yay -S gallery-tui      # build the latest stable release from source
yay -S gallery-tui-git  # build the latest git version from source
```

Homebrew:

```sh
brew install WindustH/tap/gallery-tui
```

The Homebrew stable formula downloads a prebuilt release binary. To build the
latest git version from source:

```sh
brew install --HEAD WindustH/tap/gallery-tui
```

## Documentation

Full documentation is organized under [doc/index.md](doc/index.md).
