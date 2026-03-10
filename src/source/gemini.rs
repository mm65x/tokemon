use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;

use super::json_session_source::{JsonSessionSource, JsonSessionSourceConfig};

pub struct GeminiConfig;

impl JsonSessionSourceConfig for GeminiConfig {
    const NAME: &'static str = "gemini";
    const DISPLAY_NAME: &'static str = "Gemini CLI";

    fn base_dir() -> PathBuf {
        paths::home_dir().join(".gemini")
    }

    fn accepted_types() -> &'static [&'static str] {
        &["gemini", "model", "assistant"]
    }

    fn discover_files(base_dir: &Path) -> Vec<PathBuf> {
        // Structure: tmp/{project}/chats/session-*.json
        //            tmp/{project}/session.json
        let tmp_dir = base_dir.join("tmp");
        let mut files = Vec::new();
        let Ok(projects) = fs::read_dir(&tmp_dir) else {
            return files;
        };
        for project in projects
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_dir())
        {
            let project_path = project.path();
            // Check for session.json directly in project dir
            let session_file = project_path.join("session.json");
            if session_file.is_file() {
                files.push(session_file);
            }
            // Check for session-*.json in chats/ subdir
            let chats_dir = project_path.join("chats");
            for f in super::discover::collect_by_ext(&chats_dir, "json") {
                if f.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("session"))
                {
                    files.push(f);
                }
            }
        }
        files
    }
}

pub type GeminiSource = JsonSessionSource<GeminiConfig>;
