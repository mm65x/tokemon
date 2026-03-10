//! Bounded directory walking utilities for file discovery.
//!
//! Replaces unbounded `glob("**/*.ext")` patterns with `read_dir`-based
//! traversal constrained to a known maximum depth.

use std::path::{Path, PathBuf};

/// Collect files with a given extension from a single directory (non-recursive).
#[must_use]
pub fn collect_by_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_collect(dir, 1, &|p| has_ext(p, ext), &mut results);
    results
}

/// Walk directories up to `max_depth` levels deep, collecting files
/// whose extension matches `ext`.
///
/// Depth 1 = files directly in `dir`, depth 2 = files in `dir/*/`, etc.
#[must_use]
pub fn walk_by_ext(dir: &Path, ext: &str, max_depth: usize) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_collect(dir, max_depth, &|p| has_ext(p, ext), &mut results);
    results
}

/// Core recursive walker. Enumerates entries in `dir`, collecting files
/// that satisfy `predicate` and recursing into subdirectories while
/// `depth > 1`.
fn walk_collect(
    dir: &Path,
    depth: usize,
    predicate: &dyn Fn(&Path) -> bool,
    out: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(std::result::Result::ok) {
        // Use entry.file_type() which does NOT follow symlinks,
        // avoiding infinite loops from circular symlinks.
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if ft.is_file() {
            if predicate(&path) {
                out.push(path);
            }
        } else if ft.is_dir() && depth > 1 {
            walk_collect(&path, depth - 1, predicate, out);
        }
    }
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_collect_by_ext() {
        let dir = std::env::temp_dir().join("tokemon_discover_test_ext");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.jsonl"), "").unwrap();
        fs::write(dir.join("b.jsonl"), "").unwrap();
        fs::write(dir.join("c.txt"), "").unwrap();

        let files = collect_by_ext(&dir, "jsonl");
        assert_eq!(files.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_walk_by_ext_bounded() {
        let dir = std::env::temp_dir().join("tokemon_discover_test_walk");
        let _ = fs::remove_dir_all(&dir);
        let deep = dir.join("a/b/c");
        fs::create_dir_all(&deep).unwrap();
        fs::write(dir.join("top.jsonl"), "").unwrap();
        fs::write(dir.join("a/mid.jsonl"), "").unwrap();
        fs::write(deep.join("deep.jsonl"), "").unwrap();

        // depth 1: only top
        assert_eq!(walk_by_ext(&dir, "jsonl", 1).len(), 1);
        // depth 2: top + mid
        assert_eq!(walk_by_ext(&dir, "jsonl", 2).len(), 2);
        // depth 4: all three
        assert_eq!(walk_by_ext(&dir, "jsonl", 4).len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }
}
