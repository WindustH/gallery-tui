# Cache and Logs

All of these live in the cache directory: `~/.cache/gallery-tui/` on Linux and
macOS (`$XDG_CACHE_HOME/gallery-tui/` when that variable is set) and
`%LOCALAPPDATA%\gallery-tui\` on Windows.

| Path | Contents |
| --- | --- |
| `<hash>.ansi` | A rendered image |
| `svg/<hash>.png` | An SVG rasterized at one preview size |
| `*.used` | Last-use markers for the two above |
| `logs/<date>-<time>-<pid>-<nonce>.log` | One log per run |

## Render Cache

A render passes through three cache levels before gallery-tui renders it from
the source file:

| Level | Holds | Limit |
| --- | --- | --- |
| Memory | Finished renders, ready to draw | `render.raw_memory_cache_max_bytes` (128 MiB) |
| Compressed memory | zstd-compressed renders | `render.compressed_memory_cache_max_bytes` (256 MiB) |
| Disk | The compressed files listed above | `render.disk_cache_max_bytes` (512 MiB) |

Limits are in bytes, and `0` removes a limit. The memory levels drop their least
recently used entries as they fill. The disk limit is enforced at startup, by
deleting the least recently used files until the cache fits. A file's last use
is tracked with its `.used` marker rather than file access times, which many
systems do not update.

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

deletes the `.ansi` renders, SVG rasterizations, and their `.used` markers, and
empties both memory levels. Logs and any other files are kept.

## Sharing

Several gallery-tui instances can share one cache directory. Cache files,
markers, and rewritten config files are written to a temporary file next to the
target and then renamed over it, so another instance sees either the old or
the new complete file. Cleanup tolerates files that another instance already
removed.

## Temporary Files

Drafts for `$EDITOR` are written to `/tmp/gallery-tui/editor/` on Linux and
macOS and to `%TEMP%\gallery-tui\editor\` on Windows. Set `GALLERY_TUI_TMPDIR`
to use another base directory.

## Logs

Each run writes a log at `info` level with the detected terminal capabilities,
the chosen render modes, cache cleanup results, and warnings such as failed
renders or unreadable files. It is the first place to look when an image does
not appear.
