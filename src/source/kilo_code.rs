use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::types::Record;

use super::cline_format::ClineFormat;

pub struct KiloCodeSource {
    format: ClineFormat,
}

impl Default for KiloCodeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl KiloCodeSource {
    pub fn new() -> Self {
        Self {
            format: ClineFormat {
                provider_name: "kilo-code",
                extension_id: "kilocode.kilo-code",
            },
        }
    }
}

impl super::Source for KiloCodeSource {
    fn name(&self) -> &'static str {
        "kilo-code"
    }

    fn display_name(&self) -> &'static str {
        "Kilo Code"
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
