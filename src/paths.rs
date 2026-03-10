use std::path::PathBuf;

/// VSCode-compatible editor forks that store extensions in globalStorage
const VSCODE_FORKS: &[&str] = &[
    "Code",
    "Code - Insiders",
    "Cursor",
    "Windsurf",
    "VSCodium",
    "Positron",
];

#[allow(clippy::map_unwrap_or)]
pub fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| {
            eprintln!("[tokemon] Warning: could not determine home directory");
            PathBuf::from(".")
        })
}

pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "tokemon").map_or_else(
        || home_dir().join(".cache/tokemon"),
        |d| d.cache_dir().to_path_buf(),
    )
}

pub fn vscode_global_storage_dirs() -> Vec<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support")
    } else if cfg!(target_os = "linux") {
        home_dir().join(".config")
    } else {
        directories::BaseDirs::new()
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_default()
    };

    VSCODE_FORKS
        .iter()
        .map(|fork| base.join(fork).join("User/globalStorage"))
        .filter(|p| p.exists())
        .collect()
}
