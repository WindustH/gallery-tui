# Commands

Commands are entered from the prompt opened by `:`.

Prompt controls:

- `tab`: move to the next completion candidate
- `shift-tab`: move to the previous completion candidate
- `enter`: insert the selected completion candidate when a useful completion is
  selected; otherwise run the command
- `up`, `down`: browse command history for the current session

The completion list covers command names, layout names, sort fields, visible
metadata fields, and `asc`/`desc`.

## `:refresh`

Rescan the folder and reapply the current sort.

If the focused file still exists, focus is restored to it. Selected paths that
no longer exist are removed from selection. Files that disappear or become
inaccessible during scan are skipped and logged instead of failing the whole
scan.

## `:clear-cache`

Delete rendered image cache files and their `.used` LRU markers.

This does not delete logs or unrelated files under `~/.cache/gallery-tui/`.

## `:sort <field> <asc|desc>`

Sort by a built-in field or any visible metadata tag.

Examples:

```text
:sort name asc
:sort created desc
:sort Exif.ExposureTime desc
:sort ISO asc
```

Built-in fields:

- `name`, `filename`, `file`
- `path`
- `modified`, `mtime`
- `created`, `ctime`
- `size`
- `format`, `extension`, `ext`
- `dimensions`, `resolution`
- `metadata`, `exif` for metadata tag count

Unknown fields are treated as metadata keys. Metadata values are compared as
numbers when a number or fraction can be parsed, then fall back to
case-insensitive text comparison.

## `:layout <name> [args...]`

Switch to a layout preset. Positional arguments are mapped through the selected
preset's `params` list in `config.toml`. This command writes the selected
layout and arguments back to `config.toml`, so the layout is restored on the
next startup.

Examples:

```text
:layout grid 3 3
:layout grid 3x3
:layout list 12
:layout masonry
:layout masonry 4 30
```

Default presets:

- `grid <columns> <rows>`: fixed grid that sizes cards to fill the browser area
- `list <items>`: single-column list with the requested visible items per page
- `masonry <columns> <card_width>`: dense masonry; use `0` columns for automatic
  column calculation

If fewer arguments are supplied, the preset defaults are used for the rest.
Supplying more arguments than the preset declares is an error.

## `:layout-use <name> [args...]`

Temporarily switch to a layout preset without writing `config.toml`.

The syntax and preset argument handling are the same as `:layout`.
