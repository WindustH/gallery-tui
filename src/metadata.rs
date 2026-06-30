use std::{
  collections::BTreeMap,
  fs::File,
  io::BufReader,
  path::{Path, PathBuf},
  process::Command,
};

use tracing::debug;

use crate::model::{ImageItem, ImageMetadataEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNameChange {
  pub old_value: String,
  pub new_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataChange {
  pub tag: String,
  pub old_value: Option<String>,
  pub new_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetadataEdit {
  pub file_name: Option<FileNameChange>,
  pub tags: Vec<MetadataChange>,
}

impl MetadataEdit {
  pub fn is_empty(&self) -> bool {
    self.file_name.is_none() && self.tags.is_empty()
  }

  pub fn change_count(&self) -> usize {
    usize::from(self.file_name.is_some()) + self.tags.len()
  }
}

pub fn read_image_metadata(path: &Path) -> Vec<ImageMetadataEntry> {
  match read_exif_metadata(path) {
    Ok(entries) => entries,
    Err(error) => {
      debug!(path = %path.display(), %error, "failed to read image metadata");
      Vec::new()
    }
  }
}

pub fn metadata_edit_draft(item: &ImageItem) -> String {
  let mut out = String::new();
  out.push_str("# Edit file.name and values under [tags]. Save and exit to continue.\n");
  out.push_str("# gallery-tui will ask for confirmation before writing changes.\n");
  out.push_str(&format!(
    "# file = {:?}\n\n",
    item.path.display().to_string()
  ));
  out.push_str("[file]\n");
  out.push_str(&format!("name = {}\n\n", toml_string(&item.file_name)));
  out.push_str("[tags]\n");
  for (key, value) in editable_metadata_map(&item.metadata) {
    out.push_str(&format!("{} = {}\n", toml_key(&key), toml_string(&value)));
  }
  out
}

pub fn metadata_changes_from_edit(
  original_file_name: &str,
  original: &[ImageMetadataEntry],
  edited: &str,
) -> Result<MetadataEdit, String> {
  let original = editable_metadata_map(original);
  let value = toml::from_str::<toml::Value>(edited)
    .map_err(|err| format!("metadata draft is not valid TOML: {err}"))?;

  let file_name = match value.get("file") {
    Some(file) => {
      let Some(file) = file.as_table() else {
        return Err("metadata draft [file] must be a table".to_string());
      };
      match file.get("name") {
        Some(name) => Some(
          name
            .as_str()
            .ok_or_else(|| "metadata draft file.name must be a string".to_string())?,
        ),
        None => None,
      }
    }
    None => None,
  };

  let Some(tags) = value.get("tags").and_then(toml::Value::as_table) else {
    return Err("metadata draft must contain a [tags] table".to_string());
  };

  let mut edit = MetadataEdit::default();
  if let Some(file_name) = file_name
    && file_name != original_file_name
  {
    edit.file_name = Some(FileNameChange {
      old_value: original_file_name.to_string(),
      new_value: file_name.to_string(),
    });
  }

  for (tag, value) in tags {
    let Some(value) = value.as_str() else {
      return Err(format!("metadata tag {tag} must be a string"));
    };
    let old_value = original.get(tag).cloned();
    if old_value.as_deref() != Some(value) {
      edit.tags.push(MetadataChange {
        tag: tag.clone(),
        old_value,
        new_value: value.to_string(),
      });
    }
  }
  edit.tags.sort_by(|left, right| left.tag.cmp(&right.tag));
  Ok(edit)
}

pub fn write_metadata_with_exiftool(path: &Path, changes: &[MetadataChange]) -> Result<(), String> {
  if changes.is_empty() {
    return Ok(());
  }
  let mut command = Command::new("exiftool");
  command.arg("-overwrite_original");
  for change in changes {
    let tag = exiftool_tag_name(&change.tag)?;
    command.arg(format!("-{tag}={}", change.new_value));
  }
  command.arg(path);
  let output = command
    .output()
    .map_err(|err| format!("failed to run exiftool; install exiftool or put it in PATH: {err}"))?;
  if output.status.success() {
    Ok(())
  } else {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!(
      "exiftool failed: {}{}",
      stderr.trim(),
      stdout.trim()
    ))
  }
}

pub fn refresh_metadata_after_write(path: PathBuf) -> Vec<ImageMetadataEntry> {
  read_image_metadata(&path)
}

fn editable_metadata_map(entries: &[ImageMetadataEntry]) -> BTreeMap<String, String> {
  let mut name_counts = BTreeMap::<String, usize>::new();
  for entry in entries {
    *name_counts.entry(entry.name.clone()).or_default() += 1;
  }
  entries
    .iter()
    .map(|entry| {
      let key = if name_counts.get(&entry.name).copied().unwrap_or(0) > 1 {
        format!("{}.{}", entry.group, entry.name)
      } else {
        entry.name.clone()
      };
      (key, entry.value.clone())
    })
    .collect()
}

fn exiftool_tag_name(key: &str) -> Result<String, String> {
  let tag = key.rsplit('.').next().unwrap_or(key).trim();
  if tag.is_empty()
    || !tag
      .chars()
      .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':'))
  {
    return Err(format!("unsupported metadata tag name: {key}"));
  }
  Ok(tag.to_string())
}

fn toml_key(value: &str) -> String {
  toml_string(value)
}

fn toml_string(value: &str) -> String {
  let mut out = String::with_capacity(value.len() + 2);
  out.push('"');
  for ch in value.chars() {
    match ch {
      '\\' => out.push_str("\\\\"),
      '"' => out.push_str("\\\""),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      ch if ch.is_control() => out.push(' '),
      ch => out.push(ch),
    }
  }
  out.push('"');
  out
}

fn read_exif_metadata(path: &Path) -> Result<Vec<ImageMetadataEntry>, String> {
  let file = File::open(path).map_err(|err| err.to_string())?;
  let mut reader = BufReader::new(file);
  let exif = exif::Reader::new()
    .read_from_container(&mut reader)
    .map_err(|err| err.to_string())?;

  let mut entries = Vec::new();
  for field in exif.fields() {
    let value = field.display_value().with_unit(&exif).to_string();
    if value.trim().is_empty() {
      continue;
    }
    entries.push(ImageMetadataEntry {
      group: format!("{:?}", field.ifd_num),
      name: field.tag.to_string(),
      value,
    });
  }
  entries.sort_by(|left, right| {
    left
      .group
      .cmp(&right.group)
      .then_with(|| left.name.cmp(&right.name))
  });
  Ok(entries)
}

#[cfg(test)]
#[path = "metadata/tests.rs"]
mod tests;
