# Keymap

Key bindings live in `keymap.toml` in the [config directory](configuration.md).
The file has four sections:

- `[browser]`: the browser view
- `[detail]`: the detail view
- `[global]`: both views; a key bound in `[browser]` or `[detail]` wins over
  the same key here
- `[input]`: the rename and command prompts

Each section holds a `keymap` list:

```toml
[browser]
keymap = [
  { on = "q", run = "quit", desc = "Quit gallery-tui" },
  { on = ["s", "n"], run = "sort name asc", desc = "Sort by name ascending" },
]
```

- `on` is one key, or a list of keys pressed in sequence.
- `run` is the action.
- `desc` is shown in the which-key hints and in the `f1` binding list.

When gallery-tui starts, any default action that a section does not bind at all
is added back with its default key. To move an action to another key, change its
`on` value instead of deleting the entry.

## Key Names

- Characters are written as themselves and are case-sensitive: `g` and `G` are
  different keys.
- Named keys: `enter`, `esc`, `space`, `tab`, `backtab` (shift-tab),
  `backspace`, `delete`, `insert`, `left`, `right`, `up`, `down`, `home`,
  `end`, `pgup` (or `pageup`), `pgdn` (or `pagedown`), and `f1` to `f12`.
- Modifiers: `ctrl-c`, `alt-x`.
- Yazi-style names also work: `<Enter>`, `<Esc>`, `<Space>`, `<PageDown>`,
  `<S-Tab>`, `<C-c>`, `<A-x>`, `<F1>`.

Named keys and modifiers match in any case (`Enter`, `PgDn`, `Ctrl-c`), and
common aliases work too: `return`, `escape`, `bs`, `del`, `pagedown`,
`shift-tab`.

## Actions

Browser (`[browser]`):

- `quit`, `open`
- `move_left`, `move_right`, `move_up`, `move_down`
- `page_up`, `page_down`, `home`, `end`
- `toggle_select`, `clear_selection`, `copy_paths`
- `sort <field> <asc|desc>`: the same arguments as [`:sort`](commands.md#sort-field-ascdesc)
- `layout <name> [args...]` and `layout-use <name> [args...]`: the same
  arguments as [`:layout`](commands.md#layout-name-args)

Detail (`[detail]`):

- `back`
- `move_left` (image page), `move_right` (metadata page)
- `move_up`, `move_down` (previous and next image)
- `home`, `end` (first and last image)
- `edit_metadata`

Both views (`[global]`, or either view section):

- `rename`, `command`, `help`

Prompt (`[input]`):

- `submit`, `cancel`, `help`
- `backspace`, `delete`
- `move_left`, `move_right`, `move_start`, `move_end`
- `kill_before_cursor`, `kill_after_cursor`
- `completion_next`, `completion_previous`
- `history_previous`, `history_next`
- `edit_in_editor`

An action bound in the wrong view does nothing and shows "not available here".
