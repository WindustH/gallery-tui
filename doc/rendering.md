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

Masonry cards and slides sidebar thumbnails use fixed image-local slices.
Grid and list render whole images in fixed-height, row-aligned cards. `[layout] scroll_divisor = 8`
sets the slice height limit to `max(1, viewport_rows / 8)`; each image gets
as many slices as its fitted height needs. Changing this value changes
slice granularity, not image scale. The first and last visible slices may
be cropped further at the window edges.

Preloading follows spatial navigation to predict the view after focusing nearby images and renders those
same slices, including boundary fragments. It uses `preload_ahead` and
`preload_behind`, runs only in spare worker slots, and reserves a worker for
visible images. Decoded, scaled source images are shared across slice jobs;
the raw-memory budget is split between these sources and finished renders.
Protocol slices are encoded directly from shared pixels; only the text fallback
materializes PNG slices, which share the SQLite disk budget. Slice renders
use the existing raw-memory, compressed-memory and SQLite cache tiers.


Slice geometry and prepared-source caching live in `img-tui::slicing`. The
visible draw and speculative preload use the same viewport plan and cache
identity. Concurrent slices of one image share its decode; unrelated images
can prepare concurrently. Completed renders become drawable before optional
compressed-memory/SQLite persistence. Persistence uses a bounded queue and may
skip entries under pressure; the ready memory cache is still available.
Kitty transmits PNG when it is smaller than raw pixels. The default slice divisor
is 8 and applies only to masonry panels (including the slides sidebar).

Preload completion alone does not redraw the screen. A preload requested by the visible viewport is promoted for display, and speculative failures have a five-second cooldown without becoming persistent visible errors. Both whole-image and sliced native rendering share prepared images and use the same cell-to-pixel scaling. Browser geometry is reused until its image list, configuration, terminal cell size, or viewport changes.

Command prompts, completion lists, status messages, and which-key hints use overlay surfaces: showing or dismissing them does not resize the main content viewport. Permanent navigation (the music tab bar and Calibre search header) keeps its existing position.
Kitty Unicode images are occluded through text cells without changing image geometry. Pixel protocols may repaint intersecting images when an overlay changes, then paint the overlay text last. Overlay surfaces consume pointer events instead of activating covered content.
A composite panel displaying a single grid image uses neutral card colors; the sidebar retains focus/selection markers.
