use super::cline_format::{ClineDerivedSource, ClineSourceConfig};

pub struct KiloCodeConfig;

impl ClineSourceConfig for KiloCodeConfig {
    const NAME: &'static str = "kilo-code";
    const DISPLAY_NAME: &'static str = "Kilo Code";
    const EXTENSION_ID: &'static str = "kilocode.kilo-code";
}

pub type KiloCodeSource = ClineDerivedSource<KiloCodeConfig>;
