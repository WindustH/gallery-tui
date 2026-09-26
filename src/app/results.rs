use std::{collections::HashSet, path::Path};

use tracing::{error, info};

use super::App;
use crate::{
  event::{CacheClearOutcome, ConfigSaveOutcome, MetadataWriteOutcome, RenameOutcome, ScanOutcome},
  model::{file_label, sort_images},
};

impl App {
  pub fn finish_scan(&mut self, outcome: ScanOutcome) {
    self.scan_pending = false;
    match outcome.result {
      Ok(mut images) => {
        sort_images(&mut images, &outcome.sort);
        self.images = images;
        self.metadata_fields = None;
        self.sort_spec = outcome.sort;
        self.restore_focus(outcome.preserve_focus.as_deref());
        let existing: HashSet<&Path> = self.images.iter().map(|item| item.path.as_path()).collect();
        self
          .selected
          .retain(|path| existing.contains(path.as_path()));
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
        self.apply_rename(&outcome.from, &outcome.to);
        self.set_message(format!("renamed to {}", file_label(&outcome.to)));
        info!(from = %outcome.from.display(), to = %outcome.to.display(), "rename finished");
      }
      Err(error) => {
        error!(from = %outcome.from.display(), to = %outcome.to.display(), error, "rename failed");
        self.set_message(format!("rename failed: {error}"));
      }
    }
  }

  /// Update the image list and selection after `from` was renamed to `to`.
  fn apply_rename(&mut self, from: &Path, to: &Path) {
    if let Some(item) = self.images.iter_mut().find(|item| item.path == from) {
      item.set_path(to.to_path_buf());
    }
    if self.selected.remove(from) {
      self.selected.insert(to.to_path_buf());
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
      self.apply_rename(&outcome.from, &outcome.to);
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
        self.metadata_fields = None;
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
