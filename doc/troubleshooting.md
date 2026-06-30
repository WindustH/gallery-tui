# Troubleshooting

## Raw Escape Sequences Appear

If raw image protocol bytes appear as terminal text, use a safer render mode:

- under zellij, keep `render.zellij_sixel = "off"`
- use Chafa symbols or ASCII fallback

## Images Do Not Fill Preview Space

Native protocol rendering scales images to the target preview bounds, including
upscaling small images. Existing old cache entries may need to be cleared:

```text
:clear-cache
```

## Cache Uses Too Much Space

Lower:

```toml
[render]
cache_max_bytes = 268435456
```

Or clear the cache from inside the TUI:

```text
:clear-cache
```

## Metadata Is Missing

Only available EXIF tags are shown. Some formats do not contain EXIF metadata,
and some images may have metadata stripped.
