# Commands

Press `:` to open the command prompt, type a command, and press `enter`.

While typing, a completion list offers command names, layout names, sort fields
(including the metadata tags of the loaded images), and `asc`/`desc`. `tab` and
`shift-tab` move through it, and `enter` inserts the selected candidate if that
changes the input; otherwise `enter` runs the command. `up` and `down` recall
commands from the current session.

| Command | Effect |
| --- | --- |
| `:refresh` | Rescan the folder |
| `:clear-cache` | Delete the render cache |
| `:sort <field> <asc\|desc>` | Sort the images |
| `:layout <name> [args...]` | Switch layout and save it as the startup layout |
| `:layout-use <name> [args...]` | Switch layout for this session only |
| `:help` | Show the key bindings |

## `:refresh`

Rescans the folder and applies the current sort. Focus stays on the same file
if it still exists, files that disappeared are dropped from the selection, and
images that failed to render are tried again. Files that cannot be read during
the scan are skipped and logged.

## `:clear-cache`

Deletes the cached renders and SVG rasterizations, together with their `.used`
markers, and empties the in-memory caches. Images on screen render again, and
earlier render failures are retried. Logs and other files in the cache
directory are kept. See [Cache and Logs](cache-and-logs.md).

## `:sort <field> <asc|desc>`

Sorts by a built-in field or by any metadata tag. The direction can also be
written `ascending` or `descending`.

```text
:sort name asc
:sort created desc
:sort dimensions desc
:sort DateTimeOriginal desc
:sort primary.ExposureTime asc
```

Built-in fields and their aliases:

| Field | Sorts by |
| --- | --- |
| `name`, `filename`, `file` | File name, ignoring ASCII case |
| `path` | Full path, ignoring ASCII case |
| `modified`, `mtime` | Modification time |
| `created`, `ctime` | Creation time, where the file system records it |
| `size` | File size |
| `format`, `extension`, `ext` | Extension, then name |
| `dimensions`, `dimension`, `resolution` | Pixel count, then width, then height |
| `metadata`, `exif` | Number of EXIF tags |

Any other field names a metadata tag. See
[Metadata and Sorting](metadata-and-sorting.md) for how tags are matched and
compared.

## `:layout <name> [args...]`

Switches to a layout preset and saves the choice, with its arguments, to
`config.toml` so it is used at the next start.

```text
:layout grid 3 3
:layout grid 3x3
:layout list 12
:layout masonry
:layout masonry 4 30
```

The default presets take these arguments:

- `grid <columns> <rows>`: a fixed grid whose cards fill the screen. `3x3` is
  shorthand for `3 3`.
- `list <items>`: one column showing the given number of items per screen.
- `masonry <columns> <card_width>`: columns of cards whose height follows each
  image's aspect ratio. `0` columns fits as many columns of `card_width` cells
  as the window allows.

Missing arguments keep the preset's values; extra arguments are an error.
Presets and the arguments they accept are defined in
[`config.toml`](configuration.md#layout).

## `:layout-use <name> [args...]`

Same as `:layout`, but the change lasts only until gallery-tui exits.

## `:help`

Opens the key binding list for the current view, like `f1`.
