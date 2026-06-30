# Rendering

Images are rendered on demand. A small configurable preload window around the
focused image keeps navigation responsive without rendering the whole folder.

Rendering is limited by:

- `render.max_concurrent`

Chafa fallback is controlled by:

- `render.chafa_bin`
- `render.chafa_args`
- `render.chafa_threads`

Native image protocols resize images to the target preview bounds, including
upscaling low-resolution images so they use the available preview space.

Chafa fallback uses `--scale=max` unless overridden in `render.chafa_args`.

## Render Backends

Native protocol backends:

- Kitty
- Sixel
- iTerm2

Text fallback backends:

- Chafa symbols
- ASCII symbols without color

Protocol bytes are written directly to the terminal after each Ratatui frame.
They are not inserted into Ratatui cells, which would expose raw escape
sequences as text.
