use std::{
  cmp::Ordering,
  path::PathBuf,
  time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub struct ImageItem {
  pub path: PathBuf,
  pub file_name: String,
  pub extension: String,
  pub size_bytes: u64,
  pub modified: Option<SystemTime>,
  pub created: Option<SystemTime>,
  pub dimensions: Option<(u32, u32)>,
  pub metadata: Vec<ImageMetadataEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadataEntry {
  pub group: String,
  pub name: String,
  pub value: String,
}

impl ImageItem {
  pub fn refresh_name(&mut self) {
    self.file_name = self
      .path
      .file_name()
      .map(|name| name.to_string_lossy().into_owned())
      .unwrap_or_else(|| self.path.display().to_string());
    self.extension = self
      .path
      .extension()
      .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
      .unwrap_or_default();
  }

  pub fn modified_key(&self) -> u128 {
    time_key(self.modified)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortField {
  Name,
  Path,
  Modified,
  Created,
  Size,
  Format,
  Dimensions,
  MetadataCount,
  Metadata(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
  Asc,
  Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
  pub field: SortField,
  pub direction: SortDirection,
}

impl Default for SortSpec {
  fn default() -> Self {
    Self {
      field: SortField::Name,
      direction: SortDirection::Asc,
    }
  }
}

impl SortSpec {
  pub fn parse(value: &str) -> Option<Self> {
    let lower = value.trim().to_ascii_lowercase();
    let (field, direction) = lower.rsplit_once('_')?;
    let field = match field {
      "name" | "filename" | "file" => SortField::Name,
      "path" => SortField::Path,
      "modified" | "mtime" => SortField::Modified,
      "created" | "ctime" => SortField::Created,
      "size" => SortField::Size,
      "format" | "extension" | "ext" => SortField::Format,
      "dimensions" | "dimension" | "resolution" => SortField::Dimensions,
      "metadata" | "exif" => SortField::MetadataCount,
      field if field.starts_with("metadata:") => {
        SortField::Metadata(field.trim_start_matches("metadata:").trim().to_string())
      }
      _ => return None,
    };
    let direction = match direction {
      "asc" => SortDirection::Asc,
      "desc" => SortDirection::Desc,
      _ => return None,
    };
    Some(Self { field, direction })
  }

  pub fn label(&self) -> String {
    let field = match &self.field {
      SortField::Name => "name".to_string(),
      SortField::Path => "path".to_string(),
      SortField::Modified => "modified".to_string(),
      SortField::Created => "created".to_string(),
      SortField::Size => "size".to_string(),
      SortField::Format => "format".to_string(),
      SortField::Dimensions => "dimensions".to_string(),
      SortField::MetadataCount => "metadata".to_string(),
      SortField::Metadata(key) => key.clone(),
    };
    let direction = match self.direction {
      SortDirection::Asc => "asc",
      SortDirection::Desc => "desc",
    };
    format!("{field} {direction}")
  }
}

pub fn sort_images(images: &mut [ImageItem], spec: &SortSpec) {
  images.sort_by(|a, b| {
    let ordering = match &spec.field {
      SortField::Name => a
        .file_name
        .to_ascii_lowercase()
        .cmp(&b.file_name.to_ascii_lowercase()),
      SortField::Path => a
        .path
        .to_string_lossy()
        .to_ascii_lowercase()
        .cmp(&b.path.to_string_lossy().to_ascii_lowercase()),
      SortField::Modified => compare_option_time(a.modified, b.modified),
      SortField::Created => compare_option_time(a.created, b.created),
      SortField::Size => a.size_bytes.cmp(&b.size_bytes),
      SortField::Format => a.extension.cmp(&b.extension).then_with(|| {
        a.file_name
          .to_ascii_lowercase()
          .cmp(&b.file_name.to_ascii_lowercase())
      }),
      SortField::Dimensions => compare_option_dimensions(a.dimensions, b.dimensions),
      SortField::MetadataCount => a.metadata.len().cmp(&b.metadata.len()),
      SortField::Metadata(key) => {
        compare_option_metadata_value(metadata_value(a, key), metadata_value(b, key))
      }
    };
    match spec.direction {
      SortDirection::Asc => ordering,
      SortDirection::Desc => ordering.reverse(),
    }
  });
}

pub fn metadata_value<'a>(item: &'a ImageItem, key: &str) -> Option<&'a str> {
  let requested = normalize_metadata_key(key);
  if requested.is_empty() {
    return None;
  }
  item
    .metadata
    .iter()
    .find(|entry| {
      normalize_metadata_key(&entry.name) == requested
        || normalize_metadata_key(&format!("{}.{}", entry.group, entry.name)) == requested
    })
    .map(|entry| entry.value.as_str())
}

fn compare_option_time(a: Option<SystemTime>, b: Option<SystemTime>) -> Ordering {
  match (a, b) {
    (Some(a), Some(b)) => a.cmp(&b),
    (Some(_), None) => Ordering::Less,
    (None, Some(_)) => Ordering::Greater,
    (None, None) => Ordering::Equal,
  }
}

fn compare_option_dimensions(a: Option<(u32, u32)>, b: Option<(u32, u32)>) -> Ordering {
  match (a, b) {
    (Some((aw, ah)), Some((bw, bh))) => aw
      .saturating_mul(ah)
      .cmp(&bw.saturating_mul(bh))
      .then_with(|| aw.cmp(&bw))
      .then_with(|| ah.cmp(&bh)),
    (Some(_), None) => Ordering::Less,
    (None, Some(_)) => Ordering::Greater,
    (None, None) => Ordering::Equal,
  }
}

fn compare_option_metadata_value(a: Option<&str>, b: Option<&str>) -> Ordering {
  match (a, b) {
    (Some(a), Some(b)) => compare_metadata_value(a, b),
    (Some(_), None) => Ordering::Less,
    (None, Some(_)) => Ordering::Greater,
    (None, None) => Ordering::Equal,
  }
}

fn compare_metadata_value(a: &str, b: &str) -> Ordering {
  match (parse_sort_number(a), parse_sort_number(b)) {
    (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    _ => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
  }
}

fn parse_sort_number(value: &str) -> Option<f64> {
  let trimmed = value.trim();
  if trimmed.is_empty() {
    return None;
  }
  let mut start = None;
  let mut end = 0;
  for (idx, ch) in trimmed.char_indices() {
    if start.is_none() && (ch.is_ascii_digit() || ch == '-' || ch == '+') {
      start = Some(idx);
    }
    if start.is_some() {
      if ch.is_ascii_digit() || matches!(ch, '.' | '/' | '-' | '+') {
        end = idx + ch.len_utf8();
      } else {
        break;
      }
    }
  }
  let raw = trimmed.get(start?..end)?.trim();
  if let Some((numerator, denominator)) = raw.split_once('/') {
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    if denominator == 0.0 {
      return None;
    }
    Some(numerator / denominator)
  } else {
    raw.parse::<f64>().ok()
  }
}

fn normalize_metadata_key(value: &str) -> String {
  value
    .chars()
    .filter(|ch| ch.is_alphanumeric())
    .flat_map(char::to_lowercase)
    .collect()
}

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;

fn time_key(time: Option<SystemTime>) -> u128 {
  time
    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
    .map(|duration| duration.as_nanos())
    .unwrap_or_default()
}
