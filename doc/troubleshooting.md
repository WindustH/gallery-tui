# Troubleshooting

The log of the last run is the newest file in `logs/` under the
[cache directory](cache-and-logs.md); it records the detected terminal, the
chosen render modes, and every render error.

## Images Show "render failed"

The card shows the error from each render mode that was tried.

- `failed to run chafa`: install [Chafa](https://hpjansson.org/chafa/), or set
  `render.chafa_bin` to its path. Then run `:clear-cache` to retry.
- `failed to decode` for a graphics mode followed by a Chafa error: the file is
  damaged or in a format neither decoder reads.

Failed images are retried after `:refresh` or `:clear-cache`.

## Raw Escape Sequences Appear as Text

The terminal (or a multiplexer between it and gallery-tui) does not understand
the graphics protocol that was detected. Force text output:

```sh
GALLERY_TUI_RENDER_MODES=symbols gallery-tui ~/Pictures
```

or set `render.auto_detect = false` in `config.toml`. Under Zellij, keep
`render.zellij_sixel = "off"` unless Sixel is known to work.

## Images Are Cut Off or Do Not Fill Their Cards

gallery-tui fits images using the terminal's cell size in pixels. If the
terminal does not report it, a size of 8x16 pixels is assumed. Renders cached by
older versions may also be sized differently; `:clear-cache` removes them.

## The Cache Uses Too Much Space

Lower the disk limit, which is applied at the next start:

```toml
[render]
disk_cache_max_bytes = 268435456
```

or delete the cache right away with `:clear-cache`.

## Metadata Is Missing

Only EXIF tags are read, and only from JPEG, TIFF, PNG, WebP, and HEIF-based
files. Many images, such as screenshots or files exported for the web, carry
no EXIF data.

## A Config File Was Reset

A file that no longer parses was renamed to `<name>.bak.<pid>.<timestamp>` in
the config directory and replaced with defaults. Copy your settings back from
the backup.
