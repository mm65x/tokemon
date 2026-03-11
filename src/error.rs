use thiserror::Error;

#[derive(Error, Debug)]
pub enum TokemonError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error in {file}: {source}")]
    JsonParse {
        file: String,
        source: serde_json::Error,
    },

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Provider '{0}' not found")]
    ProviderNotFound(String),

    #[error("Pricing error: {0}")]
    Pricing(String),

    #[error("Cache error: {0}")]
    Cache(String),
}

pub type Result<T> = std::result::Result<T, TokemonError>;
