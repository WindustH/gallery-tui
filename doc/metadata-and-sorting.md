# Metadata And Sorting

gallery-tui reads filesystem metadata and available EXIF tags.

Filesystem metadata includes:

- file name
- path
- format/extension
- file size
- image dimensions
- modified time
- created time

EXIF metadata is read with `kamadak-exif` and displayed on the metadata page
when available.

## Editing

In detail view, `e` opens a TOML draft in `$EDITOR`:

```toml
[file]
name = "image.jpg"

[tags]
Artist = "name"
ImageDescription = "description"
```

After the editor exits, gallery-tui shows a confirmation dialog. Confirming
applies only changed fields. `file.name` renames the image within the same
directory. Changed tags are written through the external `exiftool` command.
Install `exiftool` and keep it in `PATH` to use tag writing.

## Sorting

Short `s` keybindings only cover common sorts:

- name
- modified time
- size

Use `:sort <field> <asc|desc>` for everything else.

Examples:

```text
:sort created desc
:sort dimensions asc
:sort ISO asc
:sort Exif.ExposureTime desc
```

Metadata fields can be referenced by tag name or by the full visible label
shown on the metadata page.

Comparison behavior:

- values with parseable numbers or fractions sort numerically
- other values sort case-insensitively as text
- missing values sort after present values for ascending order

## Missing Files

If an image disappears while scanning or refreshing, that single file is skipped
and logged. The rest of the scan continues.
