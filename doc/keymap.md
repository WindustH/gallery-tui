# Keymap

Keymaps are stored in:

- `~/.config/gallery-tui/keymap.toml`

The file is split by context:

- `[browser]`
- `[detail]`
- `[input]`
- `[global]`

Default entries use compact Yazi-style TOML:

```toml
[browser]
keymap = [
  { on = "q", run = "quit", desc = "Quit gallery-tui" },
  { on = ["s", "n"], run = "sort name asc", desc = "Sort by name ascending" },
]
```

`on` can be a single key or a key sequence.

Supported key names include:

- characters such as `q`, `h`, `j`, `k`, `l`
- `enter`, `space`, `esc`, `tab`
- `left`, `right`, `up`, `down`
- `home`, `end`, `pgup`, `pgdn`
- Yazi-style names such as `<Enter>`, `<PageDown>`, `<C-c>`

## Actions

Common browser actions:

- `quit`
- `open`
- `move_left`, `move_right`, `move_up`, `move_down`
- `page_up`, `page_down`
- `home`, `end`
- `toggle_select`
- `copy_paths`
- `sort <field> <asc|desc>`
- `layout <name> [args...]`
- `layout-use <name> [args...]`

Sort actions use the same syntax as `:sort`, without the leading `:`.
Layout actions use the same syntax as `:layout` or `:layout-use`, without the
leading `:`.

Detail actions:

- `back`
- `move_left`, `move_right`
- `move_up`, `move_down`
- `edit_metadata`

Input actions:

- `cancel`, `submit`
- `backspace`, `delete`
- `move_left`, `move_right`, `move_start`, `move_end`
- `kill_before_cursor`, `kill_after_cursor`
- `completion_next`, `completion_previous`
- `history_previous`, `history_next`
- `edit_in_editor`

Global actions:

- `rename`
- `command`
