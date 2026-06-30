use tracing::{error, info};

use crate::{
  event::{CacheClearOutcome, ConfigSaveOutcome, MetadataWriteOutcome, RenameOutcome, ScanOutcome},
  model::sort_images,
};

use super::{App, file_label};

impl App {
  pub fn finish_scan(&mut self, outcome: ScanOutcome) {
    self.scan_pending = false;
    match outcome.result {
      Ok(mut images) => {
        sort_images(&mut images, &outcome.sort);
        self.images = images;
        self.sort_spec = outcome.sort.clone();
        self.restore_focus(outcome.preserve_focus.as_deref());
        self
          .selected
          .retain(|path| self.images.iter().any(|item| item.path == *path));
        self.set_message(format!("refreshed {} images", self.images.len()));
        info!(count = self.images.len(), "scan finished");
      }
      Err(error) => {
        error!(error, "scan failed");
        self.set_message(format!("refresh failed: {error}"));
      }
    }
  }

  pub fn finish_rename(&mut self, outcome: RenameOutcome) {
    match outcome.result {
      Ok(()) => {
        for item in &mut self.images {
          if item.path == outcome.from {
            item.path = outcome.to.clone();
            item.refresh_name();
            break;
          }
        }
        if self.selected.remove(&outcome.from) {
          self.selected.insert(outcome.to.clone());
        }
        self.set_message(format!("renamed to {}", file_label(&outcome.to)));
        info!(from = %outcome.from.display(), to = %outcome.to.display(), "rename finished");
      }
      Err(error) => {
        error!(from = %outcome.from.display(), to = %outcome.to.display(), error, "rename failed");
        self.set_message(format!("rename failed: {error}"));
      }
    }
  }

  pub fn finish_cache_clear(&mut self, outcome: CacheClearOutcome) {
    self.cache_clear_pending = false;
    match outcome.result {
      Ok(report) => {
        self.set_message(format!(
          "cleared cache: {} files, {}",
          report.removed_files,
          humansize::format_size(report.removed_bytes, humansize::DECIMAL)
        ));
        info!(
          removed_files = report.removed_files,
          removed_bytes = report.removed_bytes,
          "render cache cleared"
        );
      }
      Err(error) => {
        error!(error, "cache clear failed");
        self.set_message(format!("clear-cache failed: {error}"));
      }
    }
  }

  pub fn finish_config_save(&mut self, outcome: ConfigSaveOutcome) {
    match outcome.result {
      Ok(message) => {
        self.set_message(message);
        info!("config saved");
      }
      Err(error) => {
        error!(error, "config save failed");
        self.set_message(format!("config save failed: {error}"));
      }
    }
  }

  pub fn finish_metadata_write(&mut self, outcome: MetadataWriteOutcome) {
    if outcome.rename_applied {
      for item in &mut self.images {
        if item.path == outcome.from {
          item.path = outcome.to.clone();
          item.refresh_name();
          break;
        }
      }
      if self.selected.remove(&outcome.from) {
        self.selected.insert(outcome.to.clone());
      }
    }

    let current_path = if outcome.rename_applied {
      &outcome.to
    } else {
      &outcome.from
    };
    match outcome.result {
      Ok(metadata) => {
        if let Some(item) = self
          .images
          .iter_mut()
          .find(|item| item.path == *current_path)
        {
          item.metadata = metadata;
        }
        if outcome.edit.file_name.is_some() && outcome.edit.tags.is_empty() {
          self.set_message(format!("renamed to {}", file_label(&outcome.to)));
        } else {
          self.set_message(format!(
            "metadata updated: {} tag(s){}",
            outcome.edit.tags.len(),
            if outcome.edit.file_name.is_some() {
              " and filename"
            } else {
              ""
            }
          ));
        }
        info!(
          from = %outcome.from.display(),
          to = %outcome.to.display(),
          tags = outcome.edit.tags.len(),
          renamed = outcome.rename_applied,
          "metadata write finished"
        );
      }
      Err(error) => {
        error!(from = %outcome.from.display(), to = %outcome.to.display(), %error, "metadata write failed");
        if outcome.rename_applied {
          self.set_message(format!("metadata write failed after rename: {error}"));
        } else {
          self.set_message(format!("metadata write failed: {error}"));
        }
      }
    }
  }
}
