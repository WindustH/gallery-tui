# Theme

Theme settings are stored in:

- `~/.config/gallery-tui/theme.toml`

Colors use terminal ANSI colors. `reset` is supported and is the default
background.

Supported color names include:

- `reset`
- `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`
- `gray`, `dark_gray`
- `light_red`, `light_green`, `light_yellow`, `light_blue`, `light_magenta`, `light_cyan`
- compact aliases such as `darkgray`, `lightcyan`
- indexed colors such as `ansi:236`
- RGB values such as `#ffaa00`

## Main Colors

Selected items default to terminal white background with automatic foreground
contrast. Hovered items default to black-on-cyan; hovered selected items default
to black-on-green.

- `foreground`
- `background`
- `muted`
- `accent`
- `border`
- `focused_border`
- `selected_border`
- `selected_foreground`
- `selected_background`
- `hover_foreground`
- `hover_background`
- `hover_selected_foreground`
- `hover_selected_background`
- `error`

## Which-Key

Which-key hints are drawn above the status line. They use separate theme fields:

- `which_key_columns`
- `which_key_background`
- `which_key_foreground`
- `which_key_key`
- `which_key_rest`
- `which_key_description`
- `which_key_separator`
- `which_key_separator_color`
