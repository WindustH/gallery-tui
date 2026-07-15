use std::path::PathBuf;

pub use framework_tui::{
  CommandCompletion, Prompt, PromptBuffer, current_word_start, filter_completion_candidates,
};

#[derive(Debug, Clone)]
pub enum EditorRequest {
  Prompt {
    input: String,
  },
  Metadata {
    path: PathBuf,
    original: Vec<crate::model::ImageMetadataEntry>,
    draft: String,
  },
}

impl EditorRequest {
  pub fn initial_text(&self) -> &str {
    match self {
      EditorRequest::Prompt { input } => input,
      EditorRequest::Metadata { draft, .. } => draft,
    }
  }
}
