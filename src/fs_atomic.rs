//! Async adapter for the shared atomic filesystem operations.
use std::{io, path::Path};
pub async fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
  let path = path.to_owned();
  let contents = contents.as_ref().to_vec();
  tokio::task::spawn_blocking(move || framework_tui::fs::atomic_write_bytes(&path, &contents))
    .await
    .map_err(io::Error::other)?
}
