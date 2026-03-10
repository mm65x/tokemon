use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;

use super::json_session_source::{JsonSessionSource, JsonSessionSourceConfig};

pub struct QwenConfig;

impl JsonSessionSourceConfig for QwenConfig {
    const NAME: &'static str = "qwen";
    const DISPLAY_NAME: &'static str = "Qwen Code";

    fn base_dir() -> PathBuf {
        paths::home_dir().join(".qwen")
    }

    fn accepted_types() -> &'static [&'static str] {
        &["assistant", "model"]
    }

    fn discover_files(base_dir: &Path) -> Vec<PathBuf> {
        // Structure: tmp/{project}/session.json
        let tmp_dir = base_dir.join("tmp");
        let Ok(projects) = fs::read_dir(&tmp_dir) else {
            return Vec::new();
        };
        projects
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.path().join("session.json"))
            .filter(|p| p.is_file())
            .collect()
    }

    /// Qwen uses parent dir as session ID (file is always session.json)
    fn extract_session_id(path: &Path) -> Option<String> {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(String::from)
    }
}

pub type QwenSource = JsonSessionSource<QwenConfig>;
