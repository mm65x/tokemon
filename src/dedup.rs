use std::collections::HashSet;

use crate::types::Record;

#[must_use]
pub fn deduplicate(entries: Vec<Record>) -> Vec<Record> {
    let mut seen: HashSet<u64> = HashSet::with_capacity(entries.len());
    let mut result = Vec::with_capacity(entries.len());

    for entry in entries {
        if seen.insert(entry.dedup_hash()) {
            result.push(entry);
        }
    }
    result
}
