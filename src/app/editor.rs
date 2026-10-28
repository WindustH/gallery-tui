//! Requests to hand text or metadata to an external editor.
use std::path::PathBuf;

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
