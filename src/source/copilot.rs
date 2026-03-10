use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths;
use crate::types::Record;

pub struct CopilotSource;

impl Default for CopilotSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CopilotSource {
    pub fn new() -> Self {
        Self
    }
}

impl super::Source for CopilotSource {
    fn name(&self) -> &'static str {
        "copilot"
    }

    fn display_name(&self) -> &'static str {
        "GitHub Copilot"
    }

    fn data_dir(&self) -> PathBuf {
        let storage_dirs = paths::vscode_global_storage_dirs();
        if let Some(first) = storage_dirs.first() {
            // Copilot stores in workspaceStorage, not globalStorage
            first
                .parent()
                .map(|p| p.join("workspaceStorage"))
                .unwrap_or_default()
        } else {
            PathBuf::from("(VSCode workspaceStorage)")
        }
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        // Structure: workspaceStorage/{hash}/chatSessions/{uuid}.json
        // Must target chatSessions/ specifically to avoid workspace.json etc.
        let storage_dirs = paths::vscode_global_storage_dirs();
        let mut files = Vec::new();

        for storage_dir in storage_dirs {
            if let Some(parent) = storage_dir.parent() {
                let ws_storage = parent.join("workspaceStorage");
                let Ok(workspaces) = std::fs::read_dir(&ws_storage) else {
                    continue;
                };
                for ws in workspaces.filter_map(std::result::Result::ok) {
                    let chat_dir = ws.path().join("chatSessions");
                    if chat_dir.is_dir() {
                        files.extend(super::discover::collect_by_ext(&chat_dir, "json"));
                    }
                }
            }
        }
        files
    }

    fn parse_file(&self, _path: &Path) -> Result<Vec<Record>> {
        // Copilot doesn't store token counts in its session files.
        // Would need tiktoken for estimation. For PoC, return empty.
        Ok(Vec::new())
    }
}
