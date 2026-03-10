// Pedantic lint suppressions — these are intentional, not worth fixing globally.
// cast_precision_loss: inherent in u64→f64 for token/cost math
// cast_possible_truncation: controlled truncations with .min() guards
// cast_sign_loss: guarded by .max(0) before cast
// cast_possible_wrap: u64→i64 for timestamps, values well within range
// missing_errors_doc: internal functions, doc coverage tracked separately
// must_use_candidate: too strict for a CLI app
// struct_excessive_bools: CLI and config structs legitimately need many bools
// struct_field_names: field names match external API schemas (deserialization)
// doc_markdown: backtick enforcement on prose is noisy
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::doc_markdown
)]

pub mod cache;
pub mod cli;
pub mod config;
pub mod cost;
pub mod dedup;
pub mod display;
pub mod error;
pub mod pacemaker;
pub mod paths;
pub mod pipeline;
pub mod render;
pub mod rollup;
pub mod source;
pub mod timestamp;
pub mod types;
