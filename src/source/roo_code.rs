use super::cline_format::{ClineDerivedSource, ClineSourceConfig};

pub struct RooCodeConfig;

impl ClineSourceConfig for RooCodeConfig {
    const NAME: &'static str = "roo-code";
    const DISPLAY_NAME: &'static str = "Roo Code";
    const EXTENSION_ID: &'static str = "rooveterinaryinc.roo-cline";
}

pub type RooCodeSource = ClineDerivedSource<RooCodeConfig>;
