use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths;
use crate::types::Record;

pub struct PiebaldSource {
    db_path: PathBuf,
}

impl Default for PiebaldSource {
    fn default() -> Self {
        Self::new()
    }
}

impl PiebaldSource {
    pub fn new() -> Self {
        Self {
            db_path: if cfg!(target_os = "macos") {
                paths::home_dir().join("Library/Application Support/piebald/app.db")
            } else {
                paths::home_dir().join(".local/share/piebald/app.db")
            },
        }
    }
}

impl super::Source for PiebaldSource {
    fn name(&self) -> &'static str {
        "piebald"
    }

    fn display_name(&self) -> &'static str {
        "Piebald"
    }

    fn data_dir(&self) -> PathBuf {
        self.db_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        if self.db_path.exists() {
            vec![self.db_path.clone()]
        } else {
            Vec::new()
        }
    }

    fn parse_file(&self, _path: &Path) -> Result<Vec<Record>> {
        // Piebald DB schema not yet reverse-engineered.
        Ok(Vec::new())
    }
}
