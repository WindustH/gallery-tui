# Browser and Detail Views

## Browser

The browser shows the folder's images as cards on a scrolling canvas, in the
current sort order. The focused card uses the hover colors and selected cards
use the selection colors (see [Theme](theme.md)). The status line at the bottom
shows the view, the focused position, the number of selected images, the sort,
and the last message.

Five layouts are built in:

- `grid`: a fixed number of columns and rows sized to fill the window. The
  default startup layout is `grid 4 2`.
- `browse_with_preview`: a primary `grid 3 5` on the left and a following `grid 1 1` on the right, with a 2:3 split and one-cell gap (`. b`).
- `slides`: a scrolling thumbnail column beside a large preview; see below.
- `list`: one column with a fixed number of items per screen, the preview on
  the left and the filename on the right.
- `masonry`: columns of fixed-width cards whose heights follow each image's
  aspect ratio.

Grid and list use fixed-height rows and scroll by whole rows, without slicing
images. Masonry follows each image's aspect ratio and the terminal cell dimensions.
Masonry images and slides sidebar thumbnails crossing the viewport edge are
cropped at their original scale, never shrunk to fit the visible fragment.

Switch with [`:layout`](commands.md#layout-name-args) or define your own presets
in [`config.toml`](configuration.md#presets).

A card shows the image and, unless `card_style = "image_only"` or
`show_filename = false`, the filename on the top, bottom, left, or right.
Top and bottom labels use `label_lines` rows when it is set, and a share of the
card set by `image_ratio` otherwise; the built-in grid and masonry presets
reserve one line. `padding` keeps the content away from the card edge, which
also leaves a visible frame of the focus and selection colors around
borderless cards.

## Detail

Detail view shows the focused image on two pages:

- The image page scales the image to fill the available space.
- The metadata page shows a smaller preview next to the file name, path,
  format, size, dimensions, modified and created times, and the image's EXIF
  tags.

`j` and `k` switch images and keep the current page; `h` and `l` switch pages.
The image page hides the status line unless a prompt or key hint is showing.

Press `e` to edit the filename and metadata tags in `$EDITOR`. After the editor
closes, gallery-tui lists the changes and asks for confirmation before
renaming the file or writing tags. See
[Metadata and Sorting](metadata-and-sorting.md#editing).

## Slides

`:layout slides 1 3` displays a scrolling thumbnail column beside a large image.
`. p` selects `slides 1 5`. Slides composes the ordinary `slides-sidebar`
(masonry) and `slides-main` (grid) presets; it has no separate rendering path.
The main panel handles navigation and the sidebar follows its position.
Both are borderless by default, separated by one column. Click a thumbnail to
navigate the main panel. Customize each leaf's appearance or the composition's
split using [Composite layouts](configuration.md#composite-layouts).
