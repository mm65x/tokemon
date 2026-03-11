pub mod amp;
pub mod claude_code;
pub mod cline;
pub mod cline_format;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod discover;
pub mod droid;
pub mod gemini;
pub mod json_session_source;
pub mod jsonl_source;
pub mod kilo_code;
pub mod kimi;
pub mod openclaw;
pub mod opencode;
pub mod pi_agent;
pub mod piebald;
pub mod qwen;
pub mod roo_code;

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::dedup;
use crate::error::Result;
use crate::types::Record;

pub trait Source: Send + Sync {
    /// Short identifier: "claude-code", "codex", "gemini"
    fn name(&self) -> &'static str;

    /// Human-readable: "Claude Code", "Codex CLI", "Gemini CLI"
    fn display_name(&self) -> &'static str;

    /// Get the absolute path to the base data directory
    fn data_dir(&self) -> PathBuf;

    /// Return all data files for this provider on this machine
    fn discover_files(&self) -> Vec<PathBuf>;

    /// Parse one file into usage entries
    fn parse_file(&self, path: &Path) -> Result<Vec<Record>>;

    /// Parse all files in parallel with dedup.
    ///
    /// Handles individual file errors by logging a warning and continuing.
    /// Returns a `Result` only for fatal, unrecoverable errors (none currently
    /// implemented by the default).
    fn parse_all(&self) -> Result<Vec<Record>> {
        let files = self.discover_files();
        let all: Vec<Record> = files
            .par_iter()
            .flat_map(|f| {
                self.parse_file(f).unwrap_or_else(|e| {
                    eprintln!("[tokemon] Warning: failed to parse {}: {}", f.display(), e);
                    Vec::new()
                })
            })
            .collect();
        Ok(dedup::deduplicate(all))
    }
}

pub struct SourceSet {
    providers: Vec<Box<dyn Source>>,
}

impl Default for SourceSet {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceSet {
    pub fn new() -> Self {
        Self {
            providers: vec![
                Box::new(claude_code::ClaudeCodeSource::new()),
                Box::new(codex::CodexSource::new()),
                Box::new(gemini::GeminiSource::new()),
                Box::new(opencode::OpenCodeSource::new()),
                Box::new(amp::AmpSource::new()),
                Box::new(cline::ClineSource::new()),
                Box::new(roo_code::RooCodeSource::new()),
                Box::new(kilo_code::KiloCodeSource::new()),
                Box::new(copilot::CopilotSource::new()),
                Box::new(pi_agent::PiAgentSource::new()),
                Box::new(kimi::KimiSource::new()),
                Box::new(droid::DroidSource::new()),
                Box::new(openclaw::OpenClawSource::new()),
                Box::new(qwen::QwenSource::new()),
                Box::new(piebald::PiebaldSource::new()),
                Box::new(cursor::CursorSource::new()),
            ],
        }
    }

    pub fn available(&self) -> Vec<&dyn Source> {
        self.providers
            .iter()
            .filter(|p| !p.discover_files().is_empty())
            .map(std::convert::AsRef::as_ref)
            .collect()
    }

    pub fn all(&self) -> Vec<&dyn Source> {
        self.providers
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&dyn Source> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(std::convert::AsRef::as_ref)
    }
}
