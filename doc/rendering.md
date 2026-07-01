# Rendering

Images are rendered on demand. A small configurable preload window around the
focused image keeps navigation responsive without rendering the whole folder.

Rendering is limited by:

- `render.max_concurrent`

Focused images and detail previews share the same global render limit as
preloads, but preloads reserve at most `render.max_concurrent - 1` slots. This
keeps at least one slot available for the currently visible image while
background work is active.

Chafa fallback is controlled by:

- `render.chafa_bin`
- `render.chafa_args`
- `render.chafa_threads`

Native image protocols resize images to the target preview bounds, including
upscaling low-resolution images so they use the available preview space.
Native resize uses a SIMD-accelerated path for common 8-bit pixel formats and
falls back to the image crate for higher bit-depth formats. When multiple
native protocol backends are tried for the same image and size, the decoded and
resized intermediate image is shared across those attempts.

Sixel output reuses contiguous RGB/alpha buffers and parallelizes alpha index
preparation plus scanline run encoding.

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
