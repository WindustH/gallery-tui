# Controls

Key bindings are context aware. Browser-only commands do not fire in detail
view, and detail-only commands do not fire in browser view.

## Browser

- `q`: quit the program
- `enter`: open focused image detail
- `h/j/k/l`: move focus
- arrow keys: move focus
- left mouse click: focus the clicked card
- mouse wheel: move focus between images
- `pgup`, `pgdn`: page-style focus navigation
- `home`, `end`: first/last image
- `space`: toggle selection and move focus to the next image
- `r`: rename focused image; the cursor starts before the extension dot
- `c p`: output selected paths, or focused path if nothing is selected
- `s n`, `s N`: sort by name ascending/descending
- `s m`, `s M`: sort by modified time ascending/descending
- `s z`, `s S`: sort by size ascending/descending
- `:`: open command prompt

## Command Prompt

- `tab`, `shift-tab`: browse completion candidates
- `enter`: insert the selected completion or run the command
- `up`, `down`: browse command history for the current session
- `left`, `right`, `home`, `end`: move the cursor
- `ctrl-a`, `ctrl-e`: move to start/end
- `ctrl-u`, `ctrl-k`: delete before/after cursor
- `ctrl-g`: edit the input in `$EDITOR`
- `esc`: close the prompt

## Detail

- `q`: return to browser
- `h` or left arrow: show image page
- `l` or right arrow: show metadata page
- `j/k` or down/up arrows: next/previous image
- mouse wheel: next/previous image
- `e`: edit filename and visible metadata in `$EDITOR`; after saving, confirm before writing changes
- `r`: rename current image; the cursor starts before the extension dot
- `:`: open command prompt

## Confirm Dialog

- `y`: apply the pending change
- `enter`, `n`, `esc`: cancel

## Which-Key

When a key sequence prefix is active, gallery-tui shows a which-key style hint
area above the status line. The status line remains fixed at the bottom of the
footer.

Which-key layout and colors are configured in `theme.toml`.
