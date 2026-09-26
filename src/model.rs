use std::{
  cmp::Ordering,
  path::{Path, PathBuf},
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
  /// Point the item at `path` after a rename.
  pub fn set_path(&mut self, path: PathBuf) {
    self.file_name = file_label(&path);
    self.extension = file_extension(&path);
    self.path = path;
  }

  pub fn modified_key(&self) -> u128 {
    self
      .modified
      .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
      .map(|duration| duration.as_nanos())
      .unwrap_or_default()
  }
}

/// Display name of a path: its file name, or the whole path when it has none.
pub fn file_label(path: &Path) -> String {
  path
    .file_name()
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| path.display().to_string())
}

/// Lowercase extension of `path`, empty when it has none.
pub fn file_extension(path: &Path) -> String {
  path
    .extension()
    .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
    .unwrap_or_default()
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

impl SortField {
  /// A built-in field by name or alias, matched case-insensitively.
  fn builtin(name: &str) -> Option<Self> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
      "name" | "filename" | "file" => Self::Name,
      "path" => Self::Path,
      "modified" | "mtime" => Self::Modified,
      "created" | "ctime" => Self::Created,
      "size" => Self::Size,
      "format" | "extension" | "ext" => Self::Format,
      "dimensions" | "dimension" | "resolution" => Self::Dimensions,
      "metadata" | "exif" => Self::MetadataCount,
      _ => return None,
    })
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
  Asc,
  Desc,
}

impl SortDirection {
  fn parse(value: &str) -> Option<Self> {
    match value.trim().to_ascii_lowercase().as_str() {
      "asc" | "ascending" => Some(Self::Asc),
      "desc" | "descending" => Some(Self::Desc),
      _ => None,
    }
  }

  fn apply(self, ordering: Ordering) -> Ordering {
    match self {
      Self::Asc => ordering,
      Self::Desc => ordering.reverse(),
    }
  }
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
  /// Parse the `initial_sort` config form: `<field>_<asc|desc>`, where a
  /// metadata field is written `metadata:<key>`.
  pub fn parse(value: &str) -> Option<Self> {
    let lower = value.trim().to_ascii_lowercase();
    let (field, direction) = lower.rsplit_once('_')?;
    let field = match field.strip_prefix("metadata:") {
      Some(key) => SortField::Metadata(key.trim().to_string()),
      None => SortField::builtin(field)?,
    };
    Some(Self {
      field,
      direction: SortDirection::parse(direction)?,
    })
  }

  /// Parse `:sort <field> <direction>` arguments. Unknown fields name a
  /// metadata tag.
  pub fn from_command(field: &str, direction: &str) -> Option<Self> {
    let field = field.trim();
    if field.is_empty() {
      return None;
    }
    Some(Self {
      field: SortField::builtin(field).unwrap_or_else(|| SortField::Metadata(field.to_string())),
      direction: SortDirection::parse(direction)?,
    })
  }

  pub fn label(&self) -> String {
    let field = match &self.field {
      SortField::Name => "name",
      SortField::Path => "path",
      SortField::Modified => "modified",
      SortField::Created => "created",
      SortField::Size => "size",
      SortField::Format => "format",
      SortField::Dimensions => "dimensions",
      SortField::MetadataCount => "metadata",
      SortField::Metadata(key) => key,
    };
    let direction = match self.direction {
      SortDirection::Asc => "asc",
      SortDirection::Desc => "desc",
    };
    format!("{field} {direction}")
  }
}

/// Stable sort by `spec`. Images missing the sort value (no timestamp,
/// unknown dimensions, absent metadata tag) go last in either direction.
pub fn sort_images(images: &mut [ImageItem], spec: &SortSpec) {
  let direction = spec.direction;
  match &spec.field {
    SortField::Metadata(key) => {
      let requested = normalize_metadata_key(key);
      images.sort_by_cached_key(|item| MetadataSortKey {
        value: metadata_value(item, &requested).map(MetadataSortValue::parse),
        direction,
      });
    }
    SortField::Name => {
      images.sort_by(|a, b| direction.apply(cmp_ignore_ascii_case(&a.file_name, &b.file_name)))
    }
    SortField::Path => images.sort_by(|a, b| {
      direction.apply(cmp_ignore_ascii_case(
        &a.path.to_string_lossy(),
        &b.path.to_string_lossy(),
      ))
    }),
    SortField::Modified => {
      images.sort_by(|a, b| compare_present(a.modified, b.modified, direction, Ord::cmp))
    }
    SortField::Created => {
      images.sort_by(|a, b| compare_present(a.created, b.created, direction, Ord::cmp))
    }
    SortField::Size => images.sort_by(|a, b| direction.apply(a.size_bytes.cmp(&b.size_bytes))),
    SortField::Format => images.sort_by(|a, b| {
      direction.apply(
        a.extension
          .cmp(&b.extension)
          .then_with(|| cmp_ignore_ascii_case(&a.file_name, &b.file_name)),
      )
    }),
    SortField::Dimensions => images
      .sort_by(|a, b| compare_present(a.dimensions, b.dimensions, direction, compare_dimensions)),
    SortField::MetadataCount => {
      images.sort_by(|a, b| direction.apply(a.metadata.len().cmp(&b.metadata.len())))
    }
  }
}

/// Order present values by `cmp` in `direction`; missing values always last.
fn compare_present<T>(
  a: Option<T>,
  b: Option<T>,
  direction: SortDirection,
  cmp: impl FnOnce(&T, &T) -> Ordering,
) -> Ordering {
  match (a, b) {
    (Some(a), Some(b)) => direction.apply(cmp(&a, &b)),
    (Some(_), None) => Ordering::Less,
    (None, Some(_)) => Ordering::Greater,
    (None, None) => Ordering::Equal,
  }
}

/// Compare as if both strings were ASCII-lowercased, without allocating.
fn cmp_ignore_ascii_case(a: &str, b: &str) -> Ordering {
  a.bytes()
    .map(|byte| byte.to_ascii_lowercase())
    .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

fn compare_dimensions(&(aw, ah): &(u32, u32), &(bw, bh): &(u32, u32)) -> Ordering {
  aw.saturating_mul(ah)
    .cmp(&bw.saturating_mul(bh))
    .then_with(|| aw.cmp(&bw))
    .then_with(|| ah.cmp(&bh))
}

/// Value of the metadata tag matching `requested` (already normalized) by
/// tag name or by `group.name`.
fn metadata_value<'a>(item: &'a ImageItem, requested: &str) -> Option<&'a str> {
  if requested.is_empty() {
    return None;
  }
  item
    .metadata
    .iter()
    .find(|entry| {
      normalized_eq(&[&entry.name], requested)
        || normalized_eq(&[&entry.group, &entry.name], requested)
    })
    .map(|entry| entry.value.as_str())
}

fn normalized_chars<'a>(value: &'a str) -> impl Iterator<Item = char> + 'a {
  value
    .chars()
    .filter(|ch| ch.is_alphanumeric())
    .flat_map(char::to_lowercase)
}

/// Whether the concatenated normalized `parts` equal `normalized`.
fn normalized_eq(parts: &[&str], normalized: &str) -> bool {
  parts
    .iter()
    .flat_map(|part| normalized_chars(part))
    .eq(normalized.chars())
}

fn normalize_metadata_key(value: &str) -> String {
  normalized_chars(value).collect()
}

/// A metadata tag value as sorted: numbers (including fractions such as
/// exposure times) before text, text compared case-insensitively.
#[derive(Debug)]
enum MetadataSortValue {
  Number(f64),
  Text(String),
}

impl MetadataSortValue {
  fn parse(value: &str) -> Self {
    match parse_sort_number(value) {
      Some(number) => Self::Number(number),
      None => Self::Text(value.to_ascii_lowercase()),
    }
  }
}

impl Ord for MetadataSortValue {
  fn cmp(&self, other: &Self) -> Ordering {
    match (self, other) {
      (Self::Number(a), Self::Number(b)) => a.total_cmp(b),
      (Self::Number(_), Self::Text(_)) => Ordering::Less,
      (Self::Text(_), Self::Number(_)) => Ordering::Greater,
      (Self::Text(a), Self::Text(b)) => a.cmp(b),
    }
  }
}

impl PartialOrd for MetadataSortValue {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl PartialEq for MetadataSortValue {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == Ordering::Equal
  }
}

impl Eq for MetadataSortValue {}

#[derive(Debug, PartialEq, Eq)]
struct MetadataSortKey {
  value: Option<MetadataSortValue>,
  direction: SortDirection,
}

impl Ord for MetadataSortKey {
  fn cmp(&self, other: &Self) -> Ordering {
    compare_present(
      self.value.as_ref(),
      other.value.as_ref(),
      self.direction,
      |a, b| a.cmp(b),
    )
  }
}

impl PartialOrd for MetadataSortKey {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

/// The first number in `value`, e.g. `1/250` from `1/250 s`.
fn parse_sort_number(value: &str) -> Option<f64> {
  let trimmed = value.trim();
  let start = trimmed.find(|ch: char| ch.is_ascii_digit() || ch == '-' || ch == '+')?;
  let rest = &trimmed[start..];
  let end = rest
    .find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '.' | '/' | '-' | '+')))
    .unwrap_or(rest.len());
  let raw = &rest[..end];
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

#[cfg(test)]
mod tests {
  use super::*;

  fn item(name: &str, tags: &[(&str, &str)]) -> ImageItem {
    ImageItem {
      path: PathBuf::from(name),
      file_name: name.to_string(),
      extension: String::new(),
      size_bytes: 0,
      modified: None,
      created: None,
      dimensions: None,
      metadata: tags
        .iter()
        .map(|(name, value)| ImageMetadataEntry {
          group: "Primary".to_string(),
          name: name.to_string(),
          value: value.to_string(),
        })
        .collect(),
    }
  }

  fn names(images: &[ImageItem]) -> Vec<&str> {
    images.iter().map(|item| item.file_name.as_str()).collect()
  }

  #[test]
  fn metadata_sort_handles_mixed_numbers_and_text() {
    // "10.1.1" is not a number, so these values used to form an ordering
    // cycle (2 < 10 numerically, "10" < "10.1.1" < "2" as text), which
    // makes the standard library sort panic.
    let values = ["2", "10", "10.1.1", "1.2.3", "9", "abc", "1/0", "0.5"];
    let mut images: Vec<_> = (0..64)
      .map(|index| {
        let value = values[index * 7 % values.len()];
        item(&format!("{index:02}"), &[("Software", value)])
      })
      .collect();
    let spec = SortSpec::from_command("Software", "asc").unwrap();
    sort_images(&mut images, &spec);
    let sorted: Vec<_> = images
      .iter()
      .map(|item| item.metadata[0].value.as_str())
      .collect();
    let first_text = sorted
      .iter()
      .position(|value| parse_sort_number(value).is_none())
      .unwrap();
    assert!(
      sorted[first_text..]
        .iter()
        .all(|value| parse_sort_number(value).is_none())
    );
    assert_eq!(sorted[0], "0.5");
  }

  #[test]
  fn missing_values_sort_last_in_both_directions() {
    let mut images = vec![
      item("none", &[]),
      item("old", &[("DateTimeOriginal", "2020-01-01 10:00:00")]),
      item("new", &[("DateTimeOriginal", "2024-06-01 10:00:00")]),
    ];
    sort_images(
      &mut images,
      &SortSpec::from_command("DateTimeOriginal", "desc").unwrap(),
    );
    assert_eq!(names(&images), ["new", "old", "none"]);
    sort_images(
      &mut images,
      &SortSpec::from_command("Primary.DateTimeOriginal", "asc").unwrap(),
    );
    assert_eq!(names(&images), ["old", "new", "none"]);
  }

  #[test]
  fn name_sort_ignores_ascii_case() {
    let mut images = vec![item("b.png", &[]), item("A.png", &[]), item("a.jpg", &[])];
    sort_images(&mut images, &SortSpec::default());
    assert_eq!(names(&images), ["a.jpg", "A.png", "b.png"]);
  }

  #[test]
  fn sort_specs_parse_config_and_command_forms() {
    assert_eq!(
      SortSpec::parse("Modified_DESC"),
      Some(SortSpec {
        field: SortField::Modified,
        direction: SortDirection::Desc
      })
    );
    assert_eq!(
      SortSpec::parse("metadata:ExposureTime_asc").map(|spec| spec.field),
      Some(SortField::Metadata("exposuretime".to_string()))
    );
    assert_eq!(SortSpec::parse("unknown_asc"), None);
    assert_eq!(
      SortSpec::from_command("ISO", "descending").map(|spec| spec.field),
      Some(SortField::Metadata("ISO".to_string()))
    );
    assert_eq!(SortSpec::from_command("name", "up"), None);
  }

  #[test]
  fn parse_sort_number_reads_leading_numbers_and_fractions() {
    assert_eq!(parse_sort_number("1/250 s"), Some(0.004));
    assert_eq!(parse_sort_number("f/2.8"), Some(2.8));
    assert_eq!(parse_sort_number("ISO 400"), Some(400.0));
    assert_eq!(parse_sort_number("1/0"), None);
    assert_eq!(parse_sort_number("unknown"), None);
  }
}
