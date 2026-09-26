# Rendering

## Render Modes

Each image is drawn with the first render mode that works, in this order:

1. Kitty graphics protocol
2. Sixel
3. iTerm2 inline images
4. Chafa symbols: colored Unicode block characters
5. ASCII: uncolored Chafa text

Graphics protocols are used only when the terminal supports them (see
[Terminal Graphics](terminal-graphics.md)); the order can be overridden with
`GALLERY_TUI_RENDER_MODES`. With `render.auto_detect = false`, only the two
Chafa modes are used.

For the graphics protocols, gallery-tui decodes the image itself, applies EXIF
rotation and embedded ICC color profiles, and scales it to the preview size,
enlarging small images to fill the space. The Chafa modes run the external
`chafa` program with `render.chafa_args` (see
[Configuration](configuration.md#render)).

## When Images Render

Images render on demand: the cards on screen first, then a window around the
focused image (`render.preload_behind` before and `render.preload_ahead` after
it; in detail view at most one before and two after).

At most `render.max_concurrent` images render at once. Preloading starts only
when a slot is free and leaves one slot for the images on screen. When you
scroll past images faster than they render, queued renders for images that are
no longer on screen are dropped.

Finished renders are cached in memory and on disk, so revisiting an image does
not render it again. See [Cache and Logs](cache-and-logs.md).

## Formats

| Format | How it is drawn |
| --- | --- |
| JPEG, PNG, GIF, WebP, BMP, TIFF, QOI, ICO, PNM, TGA | Decoded by gallery-tui for graphics protocols, or by Chafa |
| SVG | Rasterized to PNG with resvg at the preview size, cached, then drawn like a PNG |
| AVIF | Chafa modes only, if your Chafa build reads AVIF |

Animated GIF and WebP files show their first frame. Which extensions are
scanned at all is set by `supported_extensions` in `config.toml`.
