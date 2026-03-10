use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::types::Record;

use super::cline_format::ClineFormat;

pub struct RooCodeSource {
    format: ClineFormat,
}

impl Default for RooCodeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl RooCodeSource {
    pub fn new() -> Self {
        Self {
            format: ClineFormat {
                provider_name: "roo-code",
                extension_id: "rooveterinaryinc.roo-cline",
            },
        }
    }
}

impl super::Source for RooCodeSource {
    fn name(&self) -> &'static str {
        "roo-code"
    }

    fn display_name(&self) -> &'static str {
        "Roo Code"
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
