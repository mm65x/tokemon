use super::cline_format::{ClineDerivedSource, ClineSourceConfig};

pub struct ClineConfig;

impl ClineSourceConfig for ClineConfig {
    const NAME: &'static str = "cline";
    const DISPLAY_NAME: &'static str = "Cline";
    const EXTENSION_ID: &'static str = "saoudrizwan.claude-dev";
}

pub type ClineSource = ClineDerivedSource<ClineConfig>;
