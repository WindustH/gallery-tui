# Cache and Logs

All of these live in the cache directory: `~/.cache/gallery-tui/` on Linux and
macOS (`$XDG_CACHE_HOME/gallery-tui/` when that variable is set) and
`%LOCALAPPDATA%\gallery-tui\` on Windows.

| Path | Contents |
| --- | --- |
| `cache.sqlite3` | Compressed terminal renders and SVG rasterizations as BLOBs |
| `cache.sqlite3-wal`, `cache.sqlite3-shm` | SQLite WAL and coordination files |
| `logs/<date>-<time>-<pid>-<nonce>.log` | One log per run |

## Render Cache

A render passes through three cache levels before gallery-tui renders it from
the source file:

| Level | Holds | Limit |
| --- | --- | --- |
| Memory | Finished renders, ready to draw | `render.raw_memory_cache_max_bytes` (128 MiB) |
| Compressed memory | zstd-compressed renders | `render.compressed_memory_cache_max_bytes` (256 MiB) |
| Disk | SQLite payloads | `render.disk_cache_max_bytes` (512 MiB) |

`img-tui` implements both memory tiers using moka with explicit LRU eviction
and byte weights. `0` removes a limit. Memory budgets are approximate; the
currently borrowed render and one oversized result may outlive eviction.
SQLite enforces its live-payload budget transactionally on writes and at
startup. Access timestamps replace the old `.used` files and are coarsened
to one minute. One newly written oversized payload may exceed the budget.
Physical database pages, WAL and active temporary images are extra storage;
cleanup checkpoints and incrementally vacuums reusable space.

Cached renders are keyed by the image path, size, and modification time, the
preview size, the render mode, the terminal cell size, and the render settings,
so a changed image or setting never shows a stale render. Kitty renders embed
image IDs that are unique to one run, so they are reused only within that run.

Compression is set by `render.cache_compression_level` and
`render.cache_compression_threads`.

## Clearing the Cache

```text
:clear-cache
```

clears SQLite payloads and both memory levels. Logs and other files are kept.
In-flight jobs and subsequent requests may insert new entries after clearing.

On the first SQLite-cache startup, old `.ansi` files, `.used` markers and the
`svg/` cache directory are deleted, without importing or precomputing entries.

## Sharing

Each application has a separate database. Multiple gallery-tui processes can
share the same local SQLite cache using WAL transactions and a busy timeout.
File-only consumers use private content-addressed temporary images held for
the cache session's lifetime, so deleting a database row does not invalidate
an image another process is displaying. These files are removed on normal
session shutdown.

## Temporary Files

Drafts for `$EDITOR` are written to `/tmp/gallery-tui/editor/` on Linux and
macOS and to `%TEMP%\gallery-tui\editor\` on Windows. Set `GALLERY_TUI_TMPDIR`
to use another base directory.

## Logs

Each run writes a log at `info` level with the detected terminal capabilities,
the chosen render modes, cache cleanup results, and warnings such as failed
renders or unreadable files. It is the first place to look when an image does
not appear.
