use std::path::PathBuf;

use crossterm::event::Event;

use crate::{
  cache::CacheCleanupReport,
  metadata::MetadataEdit,
  model::{ImageItem, ImageMetadataEntry, SortSpec},
  render::RenderOutcome,
};

#[derive(Debug)]
pub enum AsyncEvent {
  Input {
    event: Event,
    generation: u64,
  },
  Render(RenderOutcome),
  Scan(ScanOutcome),
  Rename(RenameOutcome),
  CacheClear(CacheClearOutcome),
  ConfigSave(ConfigSaveOutcome),
  MetadataWrite(MetadataWriteOutcome),
  /// The process received a termination signal.
  #[cfg(unix)]
  Terminate,
}

#[derive(Debug)]
pub struct ScanOutcome {
  pub result: Result<Vec<ImageItem>, String>,
  pub preserve_focus: Option<PathBuf>,
  pub sort: SortSpec,
}

#[derive(Debug)]
pub struct RenameOutcome {
  pub from: PathBuf,
  pub to: PathBuf,
  pub result: Result<(), String>,
}

#[derive(Debug)]
pub struct CacheClearOutcome {
  pub result: Result<CacheCleanupReport, String>,
}

#[derive(Debug)]
pub struct ConfigSaveOutcome {
  pub result: Result<String, String>,
}

#[derive(Debug)]
pub struct MetadataWriteOutcome {
  pub from: PathBuf,
  pub to: PathBuf,
  pub result: Result<Vec<ImageMetadataEntry>, String>,
  pub edit: MetadataEdit,
  pub rename_applied: bool,
}
