use std::{
  ffi::OsString,
  io::{self, Write},
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
  time::{SystemTime, UNIX_EPOCH},
};

use tokio::io::AsyncWriteExt;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub async fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
  if let Some(parent) = path.parent() {
    tokio::fs::create_dir_all(parent).await?;
  }
  let temporary = temporary_path(path);
  let result = write_then_rename(path, &temporary, contents.as_ref()).await;
  if result.is_err() {
    let _ = tokio::fs::remove_file(&temporary).await;
  }
  result
}

pub fn write_sync(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let temporary = temporary_path(path);
  let result = write_then_rename_sync(path, &temporary, contents.as_ref());
  if result.is_err() {
    let _ = std::fs::remove_file(&temporary);
  }
  result
}

async fn write_then_rename(path: &Path, temporary: &Path, contents: &[u8]) -> io::Result<()> {
  let mut file = tokio::fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(temporary)
    .await?;
  file.write_all(contents).await?;
  file.flush().await?;
  drop(file);
  rename_replace(temporary, path).await
}

fn write_then_rename_sync(path: &Path, temporary: &Path, contents: &[u8]) -> io::Result<()> {
  let mut file = std::fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(temporary)?;
  file.write_all(contents)?;
  file.flush()?;
  drop(file);
  rename_replace_sync(temporary, path)
}

async fn rename_replace(from: &Path, to: &Path) -> io::Result<()> {
  #[cfg(windows)]
  {
    let from = from.to_path_buf();
    let to = to.to_path_buf();
    tokio::task::spawn_blocking(move || windows_rename_replace(&from, &to))
      .await
      .map_err(|err| io::Error::other(format!("rename worker failed: {err}")))?
  }
  #[cfg(not(windows))]
  {
    tokio::fs::rename(from, to).await
  }
}

fn rename_replace_sync(from: &Path, to: &Path) -> io::Result<()> {
  #[cfg(windows)]
  {
    windows_rename_replace(from, to)
  }
  #[cfg(not(windows))]
  {
    std::fs::rename(from, to)
  }
}

#[cfg(windows)]
fn windows_rename_replace(from: &Path, to: &Path) -> io::Result<()> {
  use std::os::windows::ffi::OsStrExt;
  use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
  };

  let from = from
    .as_os_str()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect::<Vec<_>>();
  let to = to
    .as_os_str()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect::<Vec<_>>();

  let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
  let replaced = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) };
  if replaced == 0 {
    Err(io::Error::last_os_error())
  } else {
    Ok(())
  }
}

fn temporary_path(path: &Path) -> PathBuf {
  let parent = path.parent().unwrap_or_else(|| Path::new("."));
  let file_name = path
    .file_name()
    .map(OsString::from)
    .unwrap_or_else(|| OsString::from("file"));
  let nonce = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_nanos();
  let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);

  // Atomic replacement requires source and target to be on the same filesystem,
  // so these temporary files intentionally live next to the final path.
  let mut temporary_name = OsString::from(".");
  temporary_name.push(file_name);
  temporary_name.push(format!(".{}.{}.{}.tmp", std::process::id(), nonce, counter));
  parent.join(temporary_name)
}
