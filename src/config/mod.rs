//! User configuration: `config.toml`, `keymap.toml`, and `theme.toml`.

mod comments;
mod file;
mod keymap;
mod layout;
mod render;
mod theme;

use std::path::PathBuf;

#[cfg(unix)]
use std::env;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::model::SortSpec;

pub use file::write_app_config;
pub use keymap::KeymapConfig;
pub use layout::{EffectiveLayoutConfig, LayoutConfig};
pub use render::RenderConfig;
pub use theme::ThemeConfig;

#[derive(Debug, Clone)]
pub struct Settings {
  pub config: AppConfig,
  pub keymap: KeymapConfig,
  pub theme: ThemeConfig,
  pub config_path: PathBuf,
  pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
  pub recursive: bool,
  pub initial_sort: String,
  pub supported_extensions: Vec<String>,
  pub layout: LayoutConfig,
  pub render: RenderConfig,
  pub behavior: BehaviorConfig,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      recursive: false,
      initial_sort: "name_asc".to_string(),
      supported_extensions: [
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "qoi", "ico", "pnm",
        "tga", "svg",
      ]
      .into_iter()
      .map(str::to_string)
      .collect(),
      layout: LayoutConfig::default(),
      render: RenderConfig::default(),
      behavior: BehaviorConfig::default(),
    }
  }
}

impl AppConfig {
  pub fn initial_sort_spec(&self) -> SortSpec {
    SortSpec::parse(&self.initial_sort).unwrap_or_default()
  }
}

impl file::ConfigFile for AppConfig {
  fn normalize(&mut self) {
    self.layout.normalize_defaults();
  }

  fn validate(&self) -> Result<(), String> {
    self.layout.validate_active()
  }

  fn to_toml(&self) -> Result<String> {
    comments::app_config_toml(self)
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
  pub scroll_lines: u16,
  pub select_moves_focus: bool,
  pub frame_sync_navigation: bool,
}

impl Default for BehaviorConfig {
  fn default() -> Self {
    Self {
      scroll_lines: 4,
      select_moves_focus: true,
      frame_sync_navigation: true,
    }
  }
}

pub async fn load_or_create() -> Result<Settings> {
  let config_dir = app_config_dir();
  let cache_dir = app_cache_dir();

  fs::create_dir_all(&config_dir)
    .await
    .with_context(|| format!("failed to create {}", config_dir.display()))?;
  fs::create_dir_all(&cache_dir)
    .await
    .with_context(|| format!("failed to create {}", cache_dir.display()))?;

  let config_path = config_dir.join("config.toml");
  let config = file::load_or_create(&config_path, AppConfig::default()).await?;
  let keymap =
    file::load_or_create(&config_dir.join("keymap.toml"), KeymapConfig::default()).await?;
  let theme = file::load_or_create(&config_dir.join("theme.toml"), ThemeConfig::default()).await?;

  Ok(Settings {
    config,
    keymap,
    theme,
    config_path,
    cache_dir,
  })
}

fn app_config_dir() -> PathBuf {
  platform_config_dir().join("gallery-tui")
}

fn app_cache_dir() -> PathBuf {
  platform_cache_dir().join("gallery-tui")
}

#[cfg(unix)]
fn platform_config_dir() -> PathBuf {
  unix_platform_dir(
    env_path("XDG_CONFIG_HOME"),
    dirs::home_dir(),
    dirs::config_dir(),
    ".config",
    ".",
  )
}

#[cfg(not(unix))]
fn platform_config_dir() -> PathBuf {
  dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(unix)]
fn platform_cache_dir() -> PathBuf {
  unix_platform_dir(
    env_path("XDG_CACHE_HOME"),
    dirs::home_dir(),
    dirs::cache_dir(),
    ".cache",
    ".cache",
  )
}

#[cfg(not(unix))]
fn platform_cache_dir() -> PathBuf {
  dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".cache"))
}

#[cfg(unix)]
fn env_path(name: &str) -> Option<PathBuf> {
  env::var_os(name)
    .filter(|value| !value.is_empty())
    .map(PathBuf::from)
}

#[cfg(unix)]
fn unix_platform_dir(
  xdg_dir: Option<PathBuf>,
  home_dir: Option<PathBuf>,
  fallback_dir: Option<PathBuf>,
  home_child: &str,
  default_dir: &str,
) -> PathBuf {
  xdg_dir
    .filter(|path| !path.as_os_str().is_empty())
    .or_else(|| home_dir.map(|home| home.join(home_child)))
    .or(fallback_dir)
    .unwrap_or_else(|| PathBuf::from(default_dir))
}
