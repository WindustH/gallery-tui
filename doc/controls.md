# Controls

These are the default bindings. Every key can be changed in
[`keymap.toml`](keymap.md). Keys only act in the view they belong to: browser
keys do nothing in detail view and the other way round.

## Browser

| Key | Action |
| --- | --- |
| `h` `j` `k` `l`, arrow keys | Move focus |
| `pgup`, `pgdn` | Move focus by one screen of cards |
| `home` or `g g` | First image |
| `end` or `G` | Last image |
| `enter` | Open the focused image in detail view |
| `space` | Toggle selection and move focus to the next image |
| `esc` | Clear the selection |
| `c p` | Quit and print the selected paths (or the focused path) to stdout |
| `s n`, `s N` | Sort by name, ascending or descending |
| `s m`, `s M` | Sort by modified time, ascending or descending |
| `s z`, `s S` | Sort by size, ascending or descending |
| `r` | Rename the focused image |
| `:` | Open the [command prompt](commands.md) |
| `f1` | Show the key bindings |
| `q`, `ctrl-c` | Quit |

Mouse: the wheel moves focus one row up or down, and a left click focuses the
clicked card.

After pressing the first key of a sequence such as `s` or `g`, a which-key
hint above the status line lists the keys that can follow.

## Detail

| Key | Action |
| --- | --- |
| `j`, `down` | Next image |
| `k`, `up` | Previous image |
| `g g`, `G` | First image, last image |
| `h`, `left` | Show the image page |
| `l`, `right` | Show the metadata page |
| `e` | Edit the filename and metadata in `$EDITOR` |
| `r` | Rename the current image |
| `:` | Open the command prompt |
| `f1` | Show the key bindings |
| `q` | Back to the browser (quits when started on a single image without `--browser`) |

Mouse: the wheel switches to the next or previous image.

## Prompt

The same keys edit the rename prompt and the `:` command prompt.

| Key | Action |
| --- | --- |
| `enter` | Accept the selected completion, or run the command |
| `esc` | Close the prompt |
| `tab`, `shift-tab` | Next or previous completion (command prompt) |
| `up`, `down` | Previous or next command from this session's history (command prompt) |
| `left`, `right` | Move the cursor |
| `home` or `ctrl-a`, `end` or `ctrl-e` | Move to the start or end |
| `backspace`, `delete` | Delete before or under the cursor |
| `ctrl-u`, `ctrl-k` | Delete everything before or after the cursor |
| `ctrl-g` | Edit the input in `$EDITOR` |
| `f1` | Show the prompt key bindings |

When renaming, the prompt starts with the current filename and the cursor just
before the extension.

## Dialogs

- Confirmation dialog: `y` applies the change; `n`, `enter`, `esc`, or `q`
  cancels.
- Key binding list (`f1`): `f1`, `esc`, `q`, or `enter` closes it.
