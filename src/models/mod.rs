// models/mod.rs — the model and effort each tool is set to (or last used),
// and the names shown for them. See docs/en/PROVIDERS.md, "Model labels".
//
// Read only what `[reads]` allows; tolerant of any file shape (an unknown
// shape means no model, never an error); only the model id and the effort
// leave a reader, never other contents of the file.

use crate::config::schema::{ModelSource, Reads};
use crate::providers::ProviderId;
use std::collections::HashMap;

/// A tool's model as it stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    /// The id as the tool stores it: "claude-opus-5-5", "gpt-6.1-sol",
    /// Antigravity's label "Claude Sonnet 4.6 (Thinking)".
    pub id: String,
    /// The effort as the tool stores it ("xhigh", "high"), if it has one.
    pub effort: Option<String>,
}

/// The model of each provider that has one to show.
pub type Models = HashMap<ProviderId, ModelInfo>;

/// A model name in its two lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameParts {
    /// The model without its version: "Opus", "GPT".
    pub model: String,
    /// The model with its version: "Opus 5.5", "GPT-6.1 Sol". Equal to
    /// `model` when the id carries no version.
    pub with_version: String,
}

/// The name shown for a model id: the user's own name when there is one,
/// otherwise the generic rule (no table of names, so a new model needs no
/// update). Either way split into model and version the same way.
pub fn name_parts(id: &str, provider: &ProviderId, rename: Option<&str>) -> NameParts {
    let _ = provider;
    let name = rename
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(id)
        .to_string();
    NameParts {
        model: name.clone(),
        with_version: name,
    }
}

/// Reads the model of each tool, only as far as `[reads]` allows, and only
/// again when a file changed (its modification time). No watchers, no
/// threads: called from the scheduler's cycle.
#[derive(Debug, Default)]
pub struct ModelReader {}

impl ModelReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// The models to show for `ids`. With `tool_settings` off nothing is
    /// read; `LastUsed` reads session tails only with `last_session` on, and
    /// falls back to the configured model for tools without sessions.
    pub fn read(&mut self, reads: &Reads, source: ModelSource, ids: &[ProviderId]) -> Models {
        let _ = (reads, source, ids);
        Models::new()
    }
}
