# Browser And Detail Views

## Browser View

The browser view is a scrollable canvas of image cards. Cards are laid out in
folder order after applying the current sort.

The focused card is highlighted. Selected cards use a separate selected style.

Supported layout strategies:

- grid
- list
- masonry

Default preset behavior:

- `grid`: fixed columns and visible rows, for example `:layout grid 3 3`
- `list`: one column with a configured number of visible items, for example
  `:layout list 12`; the default preset uses left-aligned previews, a smaller
  image ratio, no vertical gap, and no card borders
- `masonry`: dense columns using image aspect ratios, for example
  `:layout masonry 4 30`

Cards can display:

- image only
- image plus filename

Filename position can be:

- top
- bottom
- left
- right

For top and bottom filename placement, layouts can use `label_lines` to reserve
a fixed number of filename rows. The default grid and masonry presets reserve
one row.

Cards use `padding` to keep content away from the card edge. In bordered cards,
padding is applied inside the border; in borderless cards, it leaves room for
focused and selected background colors to remain visible.

## Detail View

Detail view keeps focus on the current image and has two pages:

- image page
- metadata page

The image page shows a large preview scaled and centered to use the available
space.

The metadata page shows a preview plus filesystem metadata and available EXIF
tags.

In detail view, `e` opens the filename and editable metadata in `$EDITOR`.
After the editor exits, gallery-tui shows a confirmation dialog before applying
changed fields. Filename changes rename the image in the same directory; tag
changes use `exiftool`.

Vertical navigation switches images while preserving the current detail page.
Horizontal navigation switches detail pages.
