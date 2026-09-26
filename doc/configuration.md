# Configuration

gallery-tui reads three files from its config directory:

| File | Contents |
| --- | --- |
| `config.toml` | Scanning, layout, rendering, and behavior (this page) |
| `keymap.toml` | [Key bindings](keymap.md) |
| `theme.toml` | [Colors](theme.md) |

The config directory is `~/.config/gallery-tui/` on Linux and macOS
(`$XDG_CONFIG_HOME/gallery-tui/` when that variable is set) and
`%APPDATA%\gallery-tui\` on Windows.

Missing files are created with the defaults on startup. The generated
`config.toml` has a comment above each field.

On every start gallery-tui also fills in fields that a file lacks, such as
options added by a newer version, and writes the file back if that changed its
content. A file that no longer parses, or whose startup layout is invalid, is
renamed to `<name>.bak.<pid>.<timestamp>` and replaced with a default file.

## Top Level

| Field | Default | Meaning |
| --- | --- | --- |
| `recursive` | `false` | Also scan subdirectories. Symlinked directories are not followed. |
| `initial_sort` | `"name_asc"` | Startup sort, written `<field>_<asc\|desc>` |
| `supported_extensions` | see below | File extensions to load, without the dot, any case |

`initial_sort` accepts the [built-in sort fields](commands.md#sort-field-ascdesc)
and `metadata:<tag>` for a metadata tag, for example `modified_desc` or
`metadata:DateTimeOriginal_asc`. An unrecognized value falls back to
`name_asc`.

The default extensions are `jpg`, `jpeg`, `png`, `gif`, `webp`, `bmp`, `tif`,
`tiff`, `avif`, `qoi`, `ico`, `pnm`, `tga`, and `svg`. See
[Rendering](rendering.md#formats) for how each is drawn.

## `[layout]`

The browser shows one layout preset at a time. `[layout]` selects the startup
preset and holds card settings that presets can override.

| Field | Default | Meaning |
| --- | --- | --- |
| `active` | `"grid"` | Preset used at startup |
| `active_args` | `["4", "2"]` | Arguments for that preset, as with `:layout` |
| `gap_x`, `gap_y` | `0`, `0` | Space between cards, in cells |
| `card_style` | `"image_with_name"` | `"image_only"` hides the filename |
| `show_filename` | `true` | Show the filename |
| `filename_position` | `"bottom"` | `top`, `bottom`, `left`, or `right` |
| `image_alignment` | `"center"` | `center` or `left`, within the card |
| `image_ratio` | `0.75` | Share of the card given to the image when a filename is shown, `0.1` to `0.95` |
| `label_lines` | `0` | Filename lines for `top`/`bottom` labels; `0` sizes the label from `image_ratio` |
| `show_border` | `true` | Draw a border around each card |
| `padding` | `1` | Space between the card edge (inside the border) and its content |

`:layout` rewrites `active` and `active_args`; `:layout-use` changes the layout
without touching the file.

### Presets

Presets live under `[layout.presets.<name>]`; each name can be used with
`:layout <name>`. The three built-in presets are always present, and you can
add your own:

```toml
[layout.presets.grid]
strategy = "grid"
params = ["columns", "rows"]
columns = 3
rows = 2
label_lines = 1
show_border = false

[layout.presets.list]
strategy = "list"
params = ["items"]
items = 12
filename_position = "right"
image_alignment = "left"
image_ratio = 0.35
show_border = false
padding = 0

[layout.presets.masonry]
strategy = "masonry"
params = ["columns", "card_width"]
columns = 0
card_width = 34
label_lines = 1
show_border = false

[layout.presets.big]
strategy = "grid"
params = []
columns = 2
rows = 1
```

(Excerpt: the generated file lists every field of each preset.)

| Field | Meaning |
| --- | --- |
| `strategy` | `grid`, `list`, or `masonry` |
| `params` | Names of the fields that `:layout` arguments set, in order |
| `columns` | Grid columns; masonry columns, where `0` fits as many as the width allows |
| `rows` | Grid rows per screen |
| `items` | List items per screen |
| `card_width` | Masonry card width in cells |
| `card_height` | Masonry card height for images whose size is unknown |
| `gap_x`, `gap_y`, `card_style`, `show_filename`, `filename_position`, `image_alignment`, `image_ratio`, `label_lines`, `show_border`, `padding` | Optional overrides of the `[layout]` fields |

`params` may name any field in this table except `strategy` and `params`.
Numbers must be whole and non-negative, `image_ratio` takes a decimal, and
booleans accept `true`/`false`, `yes`/`no`, `on`/`off`, or `1`/`0`. When the
first two params are `columns` and `rows`, one `<columns>x<rows>` argument such
as `3x3` also works.

## `[render]`

| Field | Default | Meaning |
| --- | --- | --- |
| `auto_detect` | `true` | Use the detected terminal graphics support and colors. When `false`, only Chafa text output is used. |
| `chafa_bin` | `"chafa"` | Chafa executable for text output |
| `chafa_args` | see below | Arguments passed to Chafa |
| `chafa_threads` | `1` | `--threads` value for each Chafa run; `0` leaves it to Chafa |
| `max_concurrent` | `4` | Images rendered at the same time |
| `preload_ahead`, `preload_behind` | `6`, `2` | Images after and before the focused one to render in advance |
| `raw_memory_cache_max_bytes` | `134217728` (128 MiB) | Memory for finished renders |
| `compressed_memory_cache_max_bytes` | `268435456` (256 MiB) | Memory for compressed renders |
| `disk_cache_max_bytes` | `536870912` (512 MiB) | Disk space for the render cache, enforced at startup |
| `cache_compression_level` | `3` | zstd level for cached renders |
| `cache_compression_threads` | `2` | zstd worker threads; `0` compresses on the calling thread |
| `zellij_sixel` | `"off"` | Sixel inside Zellij: `off`, `auto`, or `on` (see [Terminal Graphics](terminal-graphics.md#zellij)) |

The default `chafa_args` are `--format=symbols`, `--colors=full`,
`--symbols=block`, `--animate=off`, and `--polite=on`. gallery-tui always sets
`--format`, `--probe`, `--relative`, and `--passthrough` itself, and adds
`--scale=max` unless you pass `--scale`. With `auto_detect`, `--colors` and
`--symbols` follow the terminal's color support.

A size of `0` removes that cache's limit. `cache_max_bytes` is still accepted
as an older name for `disk_cache_max_bytes`. `render.passthrough`, if present in
an older file, is ignored: tmux and GNU screen passthrough is detected
automatically.

## `[behavior]`

| Field | Default | Meaning |
| --- | --- | --- |
| `select_moves_focus` | `true` | `space` moves focus to the next image after toggling the selection |
| `frame_sync_navigation` | `true` | Apply at most one navigation step per drawn frame and drop extra queued key repeats, so holding a key does not run ahead of the screen. Set to `false` to process every repeat. |
| `scroll_lines` | `4` | Unused; kept so older files stay valid |

## Environment Variables

| Variable | Effect |
| --- | --- |
| `GALLERY_TUI_RENDER_MODES` | Override the render mode order, for example `symbols` or `kitty,symbols` (see [Terminal Graphics](terminal-graphics.md#choosing-render-modes)) |
| `GALLERY_TUI_TMPDIR` | Directory for temporary files such as editor drafts |
| `XDG_CONFIG_HOME`, `XDG_CACHE_HOME` | Base config and cache directories on Linux and macOS |
| `EDITOR`, `VISUAL` | Editor for `ctrl-g` and metadata editing; `EDITOR` is tried first, then `VISUAL`, then `vi` |
