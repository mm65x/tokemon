use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::types::Record;

use super::cline_format::ClineFormat;

pub struct ClineSource {
    format: ClineFormat,
}

impl Default for ClineSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ClineSource {
    pub fn new() -> Self {
        Self {
            format: ClineFormat {
                provider_name: "cline",
                extension_id: "saoudrizwan.claude-dev",
            },
        }
    }
}

impl super::Source for ClineSource {
    fn name(&self) -> &'static str {
        "cline"
    }

    fn display_name(&self) -> &'static str {
        "Cline"
    }

    fn data_dir(&self) -> PathBuf {
        self.format.data_dir()
    }

    fn discover_files(&self) -> Vec<PathBuf> {
        self.format.discover_files()
    }

    fn parse_file(&self, path: &Path) -> Result<Vec<Record>> {
        self.format.parse_file(path)
    }
}
