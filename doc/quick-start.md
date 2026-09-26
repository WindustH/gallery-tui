# Quick Start

Open a folder of images:

```sh
gallery-tui /path/to/images
```

Or open one image. gallery-tui scans the image's folder, focuses that image,
and starts in detail view:

```sh
gallery-tui /path/to/image.png
```

When started on a single image, `q` in detail view quits. Add `--browser` to
make `q` return to the folder browser instead:

```sh
gallery-tui --browser /path/to/image.png
```

`gallery-tui --help` lists the options and `gallery-tui --version` prints the
version.

## Requirements

- [Chafa](https://hpjansson.org/chafa/) draws images as colored text when the
  terminal has no graphics protocol, and for formats the built-in decoder
  cannot read. Without it, only Kitty, Sixel, or iTerm2 terminals show images.
- [ExifTool](https://exiftool.org/) is needed only to write metadata tags from
  the detail view.

## Basic Workflow

1. Move between images with `h`/`j`/`k`/`l`, the arrow keys, the mouse wheel,
   or a mouse click.
2. Press `enter` to open the focused image in detail view.
3. In detail view, `j`/`k` switch images and `h`/`l` switch between the image
   and metadata pages.
4. Press `q` to go back to the browser, and `q` again to quit.
5. Press `f1` in any view to list its key bindings.

## Piping Paths

The interface is drawn on stderr, so stdout stays free for output. Select images
with `space`, then press `c p` to quit and print their paths (or the focused
image's path when nothing is selected), one per line:

```sh
gallery-tui ~/Pictures > picked.txt
```

## Files

The first run creates `config.toml`, `keymap.toml`, and `theme.toml` in the
config directory, and stores the render cache and logs in the cache directory:

| | Linux and macOS | Windows |
| --- | --- | --- |
| Config | `~/.config/gallery-tui/` | `%APPDATA%\gallery-tui\` |
| Cache and logs | `~/.cache/gallery-tui/` | `%LOCALAPPDATA%\gallery-tui\` |

On Linux and macOS, `$XDG_CONFIG_HOME` and `$XDG_CACHE_HOME` replace `~/.config`
and `~/.cache` when set.
