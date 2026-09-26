# gallery-tui

`gallery-tui` is a terminal image gallery. Point it at a folder and browse the
images as cards, open any of them full size, and sort, select, or rename them
without leaving the terminal.

https://github.com/user-attachments/assets/f69182bb-49cd-429c-825d-6cae8e5c43c1

## Features

- Browse a folder as a grid, list, or masonry of image cards, with the keyboard
  or the mouse.
- Open a detail view with a large preview and the file's EXIF metadata.
- Real images in terminals with Kitty, Sixel, or iTerm2 graphics, including
  under tmux and Zellij, and colored text everywhere else.
- SVG files are drawn like any other image.
- Sort by name, date, size, dimensions, or any EXIF tag.
- Select images and print their paths for other tools, rename files, and edit
  metadata in your `$EDITOR`.
- Which-key hints and fully remappable keys.
- Fast navigation: images render in the background around the focus and are
  cached in memory and on disk.

## Usage

```sh
gallery-tui /path/to/images
gallery-tui /path/to/image.png
gallery-tui --browser /path/to/image.png
```

Opening a single image starts in detail view, where `q` quits. With
`--browser`, `q` returns to the folder instead.

`c p` quits and prints the selected paths (or the focused image's path) to
stdout, so they can be piped:

```sh
gallery-tui ~/Pictures | other-tool
```

Press `f1` in any view to see its key bindings.

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

Install [Chafa](https://hpjansson.org/chafa/) for text-mode rendering in
terminals without graphics support, and [ExifTool](https://exiftool.org/) if
you want to edit metadata tags.

## Documentation

See [doc/index.md](doc/index.md) for controls, commands, configuration, and
troubleshooting.
