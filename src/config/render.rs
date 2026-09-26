use img_tui::TerminalCapability;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
  pub chafa_bin: String,
  pub auto_detect: bool,
  pub chafa_args: Vec<String>,
  pub raw_memory_cache_max_bytes: u64,
  pub compressed_memory_cache_max_bytes: u64,
  #[serde(alias = "cache_max_bytes")]
  pub disk_cache_max_bytes: u64,
  pub cache_compression_level: i32,
  pub cache_compression_threads: u32,
  pub max_concurrent: usize,
  pub chafa_threads: usize,
  pub preload_ahead: usize,
  pub preload_behind: usize,
  pub passthrough: Option<String>,
  pub zellij_sixel: String,
}

impl Default for RenderConfig {
  fn default() -> Self {
    Self {
      chafa_bin: "chafa".to_string(),
      auto_detect: true,
      chafa_args: vec![
        "--format=symbols".to_string(),
        "--colors=full".to_string(),
        "--symbols=block".to_string(),
        "--animate=off".to_string(),
        "--polite=on".to_string(),
      ],
      raw_memory_cache_max_bytes: 128 * 1024 * 1024,
      compressed_memory_cache_max_bytes: 256 * 1024 * 1024,
      disk_cache_max_bytes: 512 * 1024 * 1024,
      cache_compression_level: 3,
      cache_compression_threads: 2,
      max_concurrent: 4,
      chafa_threads: 1,
      preload_ahead: 6,
      preload_behind: 2,
      passthrough: None,
      zellij_sixel: "off".to_string(),
    }
  }
}

impl RenderConfig {
  pub fn apply_terminal_capability(&mut self, capability: &TerminalCapability) {
    self.chafa_args.retain(|arg| {
      !arg.starts_with("--format=")
        && !arg.starts_with("--colors=")
        && !arg.starts_with("--symbols=")
        && !arg.starts_with("--passthrough=")
    });
    self
      .chafa_args
      .insert(0, capability.symbols_arg().to_string());
    self
      .chafa_args
      .insert(0, capability.colors_arg().to_string());
    self.passthrough = capability.passthrough().map(str::to_string);
  }
}
