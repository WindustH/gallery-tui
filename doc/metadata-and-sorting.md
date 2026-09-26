# Metadata and Sorting

## What Is Read

When it scans a folder, gallery-tui records for each image:

- file name, path, format (extension), and size
- modified and created times (created times need file system support)
- pixel dimensions, with EXIF rotation applied
- EXIF tags, from JPEG, TIFF, PNG, WebP, and HEIF-based files

The metadata page in detail view lists all of them. EXIF tags are labeled
`<group>.<tag>`, where the group is `primary` for the main image and `thumbnail`
for the embedded thumbnail, for example `primary.ExposureTime`. Tag names
follow the EXIF standard, so the ISO speed is `PhotographicSensitivity`.

Metadata is read once per scan. Run [`:refresh`](commands.md#refresh) after
changing files outside gallery-tui.

## Sorting

The `s` key sequences cover name, modified time, and size (see
[Controls](controls.md#browser)). `:sort <field> <asc|desc>` sorts by anything
else:

```text
:sort created desc
:sort dimensions asc
:sort DateTimeOriginal desc
:sort primary.ExposureTime asc
```

A field that is not one of the [built-in fields](commands.md#sort-field-ascdesc)
names a metadata tag. It matches a tag's name or its full `<group>.<tag>` label,
ignoring case and punctuation, so `exposure_time` also finds `ExposureTime`.

Tag values are compared as follows:

- A value that starts with a number or a fraction, such as `400`, `f/2.8`, or
  `1/250 s`, is compared by that number.
- Other values are compared as text, ignoring ASCII case, and come after all
  numeric values.
- Images without the tag (or without a creation time or known dimensions, when
  sorting by those) always come last, in either direction.

Sorting is stable: images with equal values keep their previous order.

## Editing

Press `e` in detail view to open a TOML draft of the filename and tags in
`$EDITOR`:

```toml
[file]
name = "image.jpg"

[tags]
Artist = "\"name\""
ImageDescription = "\"description\""
```

Text tags appear as they do on the metadata page, including quotes. When the
editor exits, gallery-tui compares the draft with the original and asks for
confirmation. Only changed entries are applied:

- A new `file.name` renames the image within its folder. An existing file is
  never replaced, but changing only the letter case of a name works on
  case-insensitive file systems.
- Changed tags are written with [ExifTool](https://exiftool.org/), which must
  be installed and on `PATH`. For a symlinked image, the tags are written to the
  file the link points to.

Removing a line from the draft does not delete the tag. Tags that appear in
several groups use their `<group>.<tag>` label as the key; ExifTool writes them
by tag name.

## Missing Files

A file that disappears or cannot be read during a scan is skipped and logged;
the rest of the folder still loads. An image that fails to render shows the
error in its card and is retried after `:refresh` or `:clear-cache`.
