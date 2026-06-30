use std::path::PathBuf;

use crossterm::event::Event;
use ratatui::{layout::Rect, text::Text};

use crate::{
  cache::CacheCleanupReport,
  capability::RenderMode,
  metadata::MetadataEdit,
  model::{ImageItem, SortSpec},
};

#[derive(Debug)]
pub enum AsyncEvent {
  Input(Event),
  Render(RenderOutcome),
  Scan(ScanOutcome),
  Rename(RenameOutcome),
  CacheClear(CacheClearOutcome),
  ConfigSave(ConfigSaveOutcome),
  MetadataWrite(MetadataWriteOutcome),
}

#[derive(Debug)]
pub struct RenderOutcome {
  pub cache_key: String,
  pub result: Result<RenderedImage, String>,
}

#[derive(Debug, Clone)]
pub enum RenderedImage {
  Symbols {
    mode: RenderMode,
    text: Text<'static>,
  },
  Protocol {
    mode: RenderMode,
    data: String,
    fingerprint: u64,
    erase: Option<String>,
  },
}

#[derive(Debug, Clone)]
pub struct ProtocolOverlay {
  pub area: Rect,
  pub mode: RenderMode,
  pub data: String,
  pub fingerprint: u64,
  pub erase: Option<String>,
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
  pub result: Result<Vec<crate::model::ImageMetadataEntry>, String>,
  pub edit: MetadataEdit,
  pub rename_applied: bool,
}
