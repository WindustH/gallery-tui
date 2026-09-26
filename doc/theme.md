# Theme

Colors live in `theme.toml` in the [config directory](configuration.md).

## Color Values

- `reset`: the terminal's default color
- `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`
- `gray`, `dark_gray`
- `light_red`, `light_green`, `light_yellow`, `light_blue`, `light_magenta`,
  `light_cyan`
- spellings without the underscore, such as `darkgray` or `lightcyan`, and
  `grey` for `gray`
- `ansi:<0-255>` for an indexed color, such as `ansi:236`
- `#rrggbb` for an RGB color, such as `#ffaa00`

Names are case-insensitive. An unrecognized value falls back to `reset`.

## Cards and Text

| Field | Default | Used for |
| --- | --- | --- |
| `foreground` | `white` | Text and unfocused cards |
| `background` | `reset` | Background and unfocused cards |
| `muted` | `dark_gray` | Metadata labels, dialog hints, completion ghost text, the empty-folder message |
| `accent` | `cyan` | Status line view name, prompt prefix, confirmation dialog title |
| `border` | `dark_gray` | Borders on the metadata page |
| `hover_foreground` | `black` | Focused card text |
| `hover_background` | `cyan` | Focused card background |
| `selected_foreground` | `auto` | Selected card text |
| `selected_background` | `white` | Selected card background |
| `hover_selected_foreground` | `black` | Text of a card that is focused and selected |
| `hover_selected_background` | `cyan` | Background of a card that is focused and selected |

The three card foreground fields also accept `auto`, which picks black or white,
whichever contrasts with the matching background.

## Which-Key Hints

| Field | Default | Used for |
| --- | --- | --- |
| `which_key_columns` | `3` | Maximum hint columns; fewer are used in narrow windows |
| `which_key_foreground` | `white` | Hint and completion list text |
| `which_key_key` | `light_cyan` | Keys in hints and in the `f1` list |
| `which_key_description` | `light_magenta` | Descriptions in hints and in the `f1` list |
| `which_key_separator` | `" -> "` | Text between a key and its description |
| `which_key_separator_color` | `dark_gray` | Color of that separator |

`focused_border`, `selected_border`, `error`, `which_key_background`, and
`which_key_rest` are accepted for compatibility but currently have no effect.
