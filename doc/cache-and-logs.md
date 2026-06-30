# Cache And Logs

Rendered image output is cached under:

- `~/.cache/gallery-tui/<SHA256>.ansi`

Each run writes a detailed log to:

- `~/.cache/gallery-tui/logs/<startup-time>.log`

## Cache Limit

The render cache is capped by:

- `render.cache_max_bytes`

The default is `536870912` bytes, or 512 MiB.

At startup, gallery-tui removes least-recently-used render cache files until the
cache is under the configured limit.

Cache hits update a small `.used` marker so LRU does not depend on filesystem
access-time behavior.

## Compression

Cache payloads are compressed with zstd.

Compression settings:

- `render.cache_compression_level`
- `render.cache_compression_threads`

## Clearing Cache

Use:

```text
:clear-cache
```

This deletes render cache files and `.used` markers. It does not delete logs.

## Cache Keys

The cache key and header include file identity data, render mode, output size,
and terminal cell pixel size, so incompatible renders are not reused
interchangeably.
