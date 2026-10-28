//! Filesystem name validation and no-replace renaming.
use crate::model::file_label;
use std::path::{Path, PathBuf};
fn is_safe_file_name(path: &Path, parent: &Path) -> bool {
  let Some(name) = path.file_name() else {
    return false;
  };
  path.parent() == Some(parent) && name != "." && name != ".."
}

pub(crate) fn validate_new_file_name(path: &Path, file_name: &str) -> Result<PathBuf, String> {
  if file_name.trim().is_empty() {
    return Err("filename cannot be empty".to_string());
  }
  if file_name.contains('\0') {
    return Err("filename cannot contain NUL".to_string());
  }
  let Some(parent) = path.parent() else {
    return Err("cannot rename path without parent".to_string());
  };
  let to = parent.join(file_name);
  if !is_safe_file_name(&to, parent) {
    return Err("rename must stay in the same directory".to_string());
  }
  if to != path && entry_exists(&to) && !is_case_only_rename(path, &to) {
    return Err(format!("target already exists: {}", file_label(&to)));
  }
  Ok(to)
}

/// Whether anything, including a dangling symlink, exists at `path`.
fn entry_exists(path: &Path) -> bool {
  std::fs::symlink_metadata(path).is_ok()
}

/// Whether `to` differs from `from` only in letter case and both name the
/// same file, as on the case-insensitive file systems macOS and Windows use
/// by default. Such a rename must not be refused as "target exists".
fn is_case_only_rename(from: &Path, to: &Path) -> bool {
  let (Some(from_name), Some(to_name)) = (from.file_name(), to.file_name()) else {
    return false;
  };
  from_name != to_name
    && from_name.to_string_lossy().to_lowercase() == to_name.to_string_lossy().to_lowercase()
    && same_file(from, to)
}

#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
  use std::os::unix::fs::MetadataExt;
  match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
    (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
    _ => false,
  }
}

#[cfg(not(unix))]
fn same_file(a: &Path, b: &Path) -> bool {
  // Canonical paths carry the on-disk letter case, so two spellings of one
  // file resolve to the same path.
  match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
    (Ok(a), Ok(b)) => a == b,
    _ => false,
  }
}

pub(crate) fn rename_file_no_replace(from: &Path, to: &Path) -> Result<(), String> {
  if from == to {
    return Ok(());
  }
  if is_case_only_rename(from, to) {
    // `to` is `from` itself, so there is nothing to protect from replacement.
    return std::fs::rename(from, to).map_err(|err| err.to_string());
  }
  rename_file_no_replace_impl(from, to)
}

#[cfg(target_os = "linux")]
fn rename_file_no_replace_impl(from: &Path, to: &Path) -> Result<(), String> {
  use std::{ffi::CString, os::unix::ffi::OsStrExt};

  let from_c = CString::new(from.as_os_str().as_bytes())
    .map_err(|_| "source path cannot contain NUL".to_string())?;
  let to_bytes = to.as_os_str().as_bytes();
  let to_c = CString::new(to_bytes).map_err(|_| "target path cannot contain NUL".to_string())?;

  let result = unsafe {
    libc::syscall(
      libc::SYS_renameat2,
      libc::AT_FDCWD,
      from_c.as_ptr(),
      libc::AT_FDCWD,
      to_c.as_ptr(),
      libc::RENAME_NOREPLACE,
    )
  };
  if result == 0 {
    return Ok(());
  }

  let error = std::io::Error::last_os_error();
  match error.raw_os_error() {
    Some(libc::EEXIST) => Err(format!("target already exists: {}", file_label(to))),
    Some(libc::ENOSYS) | Some(libc::EINVAL) => rename_file_no_replace_fallback(from, to),
    _ => Err(error.to_string()),
  }
}

#[cfg(not(target_os = "linux"))]
fn rename_file_no_replace_impl(from: &Path, to: &Path) -> Result<(), String> {
  rename_file_no_replace_fallback(from, to)
}

fn rename_file_no_replace_fallback(from: &Path, to: &Path) -> Result<(), String> {
  if entry_exists(to) {
    return Err(format!("target already exists: {}", file_label(to)));
  }
  std::fs::rename(from, to).map_err(|err| err.to_string())
}
