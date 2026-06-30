use tokio::sync::mpsc;
use tracing::info;

use crate::{
  cache, config,
  event::{AsyncEvent, CacheClearOutcome, ConfigSaveOutcome, RenameOutcome, ScanOutcome},
  model::{SortSpec, sort_images},
  scanner,
};

use super::{App, validate_new_file_name};

impl App {
  fn apply_sort(&mut self, sort_spec: SortSpec) {
    let focused_path = self.current().map(|item| item.path.clone());
    sort_images(&mut self.images, &sort_spec);
    self.sort_spec = sort_spec.clone();
    self.restore_focus(focused_path.as_deref());
    self.set_message(format!("sort: {}", sort_spec.label()));
    info!(sort = sort_spec.label(), "sort changed");
  }

  fn request_refresh(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.scan_pending {
      self.set_message("refresh already running");
      return;
    }
    self.scan_pending = true;
    let root = self.root.clone();
    let config = self.settings.config.clone();
    let sort = self.sort_spec.clone();
    let preserve_focus = self.current().map(|item| item.path.clone());
    let tx = tx.clone();
    info!(root = %root.display(), "refresh requested");
    tokio::spawn(async move {
      let result = scanner::scan_images(root, &config)
        .await
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::Scan(ScanOutcome {
        result,
        preserve_focus,
        sort,
      }));
    });
  }

  pub(super) fn request_rename(&mut self, input: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let Some(item) = self.current() else {
      return;
    };
    let from = item.path.clone();
    let to = match validate_new_file_name(&from, &input) {
      Ok(to) => to,
      Err(error) => {
        self.set_message(error);
        return;
      }
    };
    if from == to {
      self.set_message("rename unchanged");
      return;
    }
    let tx = tx.clone();
    info!(from = %from.display(), to = %to.display(), "rename requested");
    tokio::spawn(async move {
      let result = tokio::fs::rename(&from, &to)
        .await
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::Rename(RenameOutcome { from, to, result }));
    });
  }

  pub(super) fn execute_command(&mut self, input: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let command = input.trim().trim_start_matches(':');
    let mut parts = command.split_whitespace();
    match parts.next() {
      Some("refresh") if parts.next().is_none() => self.request_refresh(tx),
      Some("clear-cache") if parts.next().is_none() => self.request_clear_cache(tx),
      Some("sort") => self.execute_sort_command(parts.collect()),
      Some("layout") => self.execute_layout_command(parts.collect(), true, tx),
      Some("layout-use") => self.execute_layout_command(parts.collect(), false, tx),
      Some("") | None => self.set_message("empty command"),
      Some(other) => self.set_message(format!("unknown command: {other}")),
    }
  }

  fn execute_sort_command(&mut self, args: Vec<&str>) {
    if args.len() < 2 {
      self.set_message("usage: :sort <field> <asc|desc>");
      return;
    }
    let direction = args[args.len() - 1];
    let field = args[..args.len() - 1].join(" ");
    let Some(sort_spec) = config::sort_for_command(&field, direction) else {
      self.set_message("usage: :sort <field> <asc|desc>");
      return;
    };
    self.apply_sort(sort_spec);
  }

  fn execute_layout_command(
    &mut self,
    args: Vec<&str>,
    persist: bool,
    tx: &mpsc::UnboundedSender<AsyncEvent>,
  ) {
    let Some(name) = args.first().copied() else {
      self.set_message("usage: :layout <name> [args...]");
      return;
    };
    match self
      .settings
      .config
      .layout
      .set_active_from_args(name, &args[1..])
    {
      Ok(layout) => {
        self.last_layout = None;
        let label = layout.label();
        if persist {
          self.set_message(format!("layout: {label} (saving)"));
          self.request_config_save(format!("layout saved: {label}"), tx);
        } else {
          self.set_message(format!("layout: {label} (temporary)"));
        }
        info!(layout = label, persist, "layout changed");
      }
      Err(error) => self.set_message(error),
    }
  }

  pub(super) fn submit_command(&mut self, input: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let command = input.trim().trim_start_matches(':').trim().to_string();
    if !command.is_empty() && self.command_history.last() != Some(&command) {
      self.command_history.push(command.clone());
    }
    self.command_history_index = None;
    self.command_history_draft = None;
    self.execute_command(command, tx);
  }

  fn request_config_save(&self, success_message: String, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    let path = self.settings.config_path.clone();
    let config = self.settings.config.clone();
    let tx = tx.clone();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
      handle.spawn(async move {
        let result = config::write_app_config(&path, &config)
          .await
          .map(|()| success_message)
          .map_err(|err| err.to_string());
        let _ = tx.send(AsyncEvent::ConfigSave(ConfigSaveOutcome { result }));
      });
    } else {
      let result = config::write_app_config_sync(&path, &config)
        .map(|()| success_message)
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::ConfigSave(ConfigSaveOutcome { result }));
    }
  }

  fn request_clear_cache(&mut self, tx: &mpsc::UnboundedSender<AsyncEvent>) {
    if self.cache_clear_pending {
      self.set_message("clear-cache already running");
      return;
    }
    self.cache_clear_pending = true;
    let cache_dir = self.settings.cache_dir.clone();
    let tx = tx.clone();
    info!(cache_dir = %cache_dir.display(), "clear-cache requested");
    tokio::spawn(async move {
      let result = cache::clear_render_cache(&cache_dir)
        .await
        .map_err(|err| err.to_string());
      let _ = tx.send(AsyncEvent::CacheClear(CacheClearOutcome { result }));
    });
  }
}
