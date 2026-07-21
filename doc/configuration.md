# Configuration

Default configuration files are created on first run:

- `~/.config/gallery-tui/config.toml`
- `~/.config/gallery-tui/keymap.toml`
- `~/.config/gallery-tui/theme.toml`

Generated `config.toml` files include comments for the available options. When
new fields are added later, `gallery-tui` writes the missing defaults back using
the same commented format; repeated preset fields share one explanation instead
of duplicating the same comment for every preset.

Existing files are normalized when they only miss fields introduced by a newer
version. If a configuration file cannot be parsed or the active layout is no
longer compatible, gallery-tui backs it up as `*.bak.<timestamp>` and writes a
fresh default file.

## `config.toml`

Top-level fields:

- `recursive`: scan subdirectories when true
- `initial_sort`: initial sort spec, such as `name_asc`
- `supported_extensions`: image extensions to scan

## `[layout]`

Layout has one active preset plus shared card style fields:

- `active`: preset name to use at startup
- `active_args`: optional positional arguments for the active preset
- `gap_x`, `gap_y`: spacing between cards
- `card_style`: `image_only` or image with filename
- `show_filename`: show or hide filename
- `filename_position`: `top`, `bottom`, `left`, or `right`
- `image_alignment`: `center` or `left`
- `image_ratio`: proportion of card content reserved for the image, from `0.1` to `0.95`
- `label_lines`: fixed filename line count for top/bottom labels; `0` uses `image_ratio`
- `show_border`: show or hide card borders
- `padding`: content padding inside cards; in bordered cards this is applied inside the border
- `presets`: named layouts available to `:layout`

Each preset has a `strategy` and a `params` list. `params` defines which
positional command arguments are accepted and how they map onto preset fields.
Running `:layout` updates `active` and `active_args` in this file. Running
`:layout-use` changes only the current session.

Default presets:

```toml
[layout]
active = "grid"
active_args = ["4", "2"]
gap_x = 0
gap_y = 0
show_border = true
padding = 1

[layout.presets.grid]
strategy = "grid"
params = ["columns", "rows"]
columns = 3
rows = 2
gap_x = 0
gap_y = 0
label_lines = 1
show_border = false
padding = 1

[layout.presets.list]
strategy = "list"
params = ["items"]
items = 12
gap_y = 0
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
padding = 1
```

Supported preset fields:

- `strategy`: `grid`, `list`, or `masonry`
- `params`: positional parameter names accepted by `:layout`
- `columns`: fixed column count, or `0` for masonry automatic columns
- `rows`: visible rows for grid layouts
- `items`: visible items per page for list layouts
- `card_width`, `card_height`: fallback card dimensions in terminal cells
- `gap_x`, `gap_y`: optional preset-specific spacing overrides
- `card_style`, `show_filename`, `filename_position`: optional preset-specific
  style overrides
- `image_alignment`: optional preset-specific preview alignment override
- `image_ratio`: optional preset-specific image/text space ratio override
- `label_lines`: optional preset-specific fixed top/bottom filename height
- `show_border`: optional preset-specific border override
- `padding`: optional preset-specific content padding override

When an existing file only misses newer fields, gallery-tui writes the parsed
defaults back into the file. When a file is incompatible, the old file is
backed up and replaced with a fresh default file.

## `[render]`

Render fields:

- `chafa_bin`: Chafa executable
- `auto_detect`: detect terminal graphics support
- `chafa_args`: extra Chafa fallback arguments
- `max_concurrent`: maximum concurrent render tasks
- `chafa_threads`: Chafa threads per process
- `preload_ahead`, `preload_behind`: preloading window around focus
- `raw_memory_cache_max_bytes`: L1 decoded render cache size limit
- `compressed_memory_cache_max_bytes`: L2 compressed in-memory cache size limit
- `disk_cache_max_bytes`: L3 disk render cache size limit
- `cache_compression_level`: zstd compression level
- `cache_compression_threads`: zstd compression threads
- `zellij_sixel`: `off`, `auto`, or `on`

`max_concurrent` is the global render worker limit. Preloading uses the same
limit but leaves one worker slot free for the focused image when possible.
Cache size values use bytes. A value of `0` disables that tier's size limit.
`cache_max_bytes` is still accepted as an old name for `disk_cache_max_bytes`.

## `[behavior]`

Behavior fields:

- `scroll_lines`: retained for scroll behavior compatibility
- `select_moves_focus`: move focus to the next image after `space`
- `frame_sync_navigation`: when true, accept at most one browser/detail
  navigation step before the next terminal frame is drawn; set to false for
  lower-latency repeated key handling on fast render paths
