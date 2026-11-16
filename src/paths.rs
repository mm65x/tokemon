use std::env;
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

const ENV_HOME: &str = "TOKEMON_HOME";
const ENV_CACHE_DIR: &str = "TOKEMON_CACHE_DIR";
const ENV_CONFIG_DIR: &str = "TOKEMON_CONFIG_DIR";

#[must_use]
fn non_empty_path(value: Option<&str>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

#[must_use]
fn resolve_home_dir(env_home: Option<&str>) -> PathBuf {
    if let Some(home) = non_empty_path(env_home) {
        return home;
    }

    directories::BaseDirs::new().map_or_else(
        || {
            eprintln!("[tokemon] Warning: could not determine home directory");
            PathBuf::from(".")
        },
        |d| d.home_dir().to_path_buf(),
    )
}

#[must_use]
fn resolve_cache_dir(env_cache_dir: Option<&str>, env_home: Option<&str>) -> PathBuf {
    if let Some(cache) = non_empty_path(env_cache_dir) {
        return cache;
    }
    if let Some(home) = non_empty_path(env_home) {
        return home.join(".cache/tokemon");
    }

    directories::ProjectDirs::from("", "", "tokemon").map_or_else(
        || home_dir().join(".cache/tokemon"),
        |d| d.cache_dir().to_path_buf(),
    )
}

#[must_use]
fn resolve_config_dir(env_config_dir: Option<&str>, env_home: Option<&str>) -> PathBuf {
    if let Some(config) = non_empty_path(env_config_dir) {
        return config;
    }
    if let Some(home) = non_empty_path(env_home) {
        return home.join(".config/tokemon");
    }

    directories::ProjectDirs::from("", "", "tokemon").map_or_else(
        || home_dir().join(".config/tokemon"),
        |d| d.config_dir().to_path_buf(),
    )
}

pub fn home_dir() -> PathBuf {
    let env_home = env::var(ENV_HOME).ok();
    resolve_home_dir(env_home.as_deref())
}

pub fn cache_dir() -> PathBuf {
    let env_cache = env::var(ENV_CACHE_DIR).ok();
    let env_home = env::var(ENV_HOME).ok();
    resolve_cache_dir(env_cache.as_deref(), env_home.as_deref())
}

pub fn config_dir() -> PathBuf {
    let env_config = env::var(ENV_CONFIG_DIR).ok();
    let env_home = env::var(ENV_HOME).ok();
    resolve_config_dir(env_config.as_deref(), env_home.as_deref())
}

pub fn vscode_global_storage_dirs() -> Vec<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support")
    } else if cfg!(target_os = "linux") {
        home_dir().join(".config")
    } else if let Some(home) = non_empty_path(env::var(ENV_HOME).ok().as_deref()) {
        home.join("AppData/Roaming")
    } else {
        directories::BaseDirs::new().map_or_else(PathBuf::new, |d| d.data_dir().to_path_buf())
    };

    VSCODE_FORKS
        .iter()
        .map(|fork| base.join(fork).join("User/globalStorage"))
        .filter(|p| p.exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_home_dir_override_and_ignores_empty() {
        let custom = resolve_home_dir(Some("/tmp/tokemon-sandbox"));
        assert_eq!(custom, PathBuf::from("/tmp/tokemon-sandbox"));

        let fallback = resolve_home_dir(Some(""));
        assert_eq!(fallback, resolve_home_dir(None));
    }

    #[test]
    fn resolves_cache_dir_with_explicit_and_home_precedence() {
        let explicit = resolve_cache_dir(Some("/tmp/explicit-cache"), Some("/tmp/tokemon-sandbox"));
        assert_eq!(explicit, PathBuf::from("/tmp/explicit-cache"));

        let from_home = resolve_cache_dir(None, Some("/tmp/tokemon-sandbox"));
        assert_eq!(
            from_home,
            PathBuf::from("/tmp/tokemon-sandbox/.cache/tokemon")
        );

        let empty_explicit_uses_home = resolve_cache_dir(Some(""), Some("/tmp/tokemon-sandbox"));
        assert_eq!(
            empty_explicit_uses_home,
            PathBuf::from("/tmp/tokemon-sandbox/.cache/tokemon")
        );
    }

    #[test]
    fn resolves_config_dir_with_explicit_and_home_precedence() {
        let explicit =
            resolve_config_dir(Some("/tmp/explicit-config"), Some("/tmp/tokemon-sandbox"));
        assert_eq!(explicit, PathBuf::from("/tmp/explicit-config"));

        let from_home = resolve_config_dir(None, Some("/tmp/tokemon-sandbox"));
        assert_eq!(
            from_home,
            PathBuf::from("/tmp/tokemon-sandbox/.config/tokemon")
        );

        let empty_explicit_uses_home = resolve_config_dir(Some(""), Some("/tmp/tokemon-sandbox"));
        assert_eq!(
            empty_explicit_uses_home,
            PathBuf::from("/tmp/tokemon-sandbox/.config/tokemon")
        );
    }
}
