//! Usage history: items used often and recently rank higher.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;
const PRUNE_AFTER: u64 = 90 * DAY;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub count: u32,
    pub last_used_unix: u64,
}

#[derive(Debug)]
pub struct Frecency {
    path: PathBuf,
    entries: HashMap<String, Entry>,
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn age_factor(age_secs: u64) -> f64 {
    match age_secs {
        a if a < HOUR => 4.0,
        a if a < DAY => 2.0,
        a if a < WEEK => 0.5,
        _ => 0.25,
    }
}

impl Frecency {
    /// Never fails: an unreadable or corrupt file yields an empty store plus a
    /// warning for the log.
    pub fn load(path: PathBuf) -> (Self, Option<String>) {
        let (entries, warning) = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(entries) => (entries, None),
                Err(err) => (HashMap::new(), Some(format!("frecency.json is corrupt, starting fresh: {err}"))),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (HashMap::new(), None),
            Err(err) => (HashMap::new(), Some(format!("cannot read frecency.json: {err}"))),
        };
        (Self { path, entries }, warning)
    }

    pub fn score(&self, id: &str, now: u64) -> f64 {
        self.entries
            .get(id)
            .map(|e| f64::from(e.count) * age_factor(now.saturating_sub(e.last_used_unix)))
            .unwrap_or(0.0)
    }

    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.entries.entry(id.to_string()).or_insert(Entry { count: 0, last_used_unix: now });
        entry.count = entry.count.saturating_add(1);
        entry.last_used_unix = now;
    }

    /// Prunes stale entries, then writes atomically (temp file + rename).
    pub fn save(&mut self, now: u64) -> std::io::Result<()> {
        self.entries.retain(|_, e| now.saturating_sub(e.last_used_unix) < PRUNE_AFTER);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&self.entries)?)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn age_factor_buckets() {
        assert_eq!(age_factor(0), 4.0);
        assert_eq!(age_factor(HOUR - 1), 4.0);
        assert_eq!(age_factor(HOUR), 2.0);
        assert_eq!(age_factor(DAY - 1), 2.0);
        assert_eq!(age_factor(DAY), 0.5);
        assert_eq!(age_factor(WEEK - 1), 0.5);
        assert_eq!(age_factor(WEEK), 0.25);
    }

    #[test]
    fn missing_file_is_empty_without_warning() {
        let dir = tempfile::tempdir().unwrap();
        let (f, warning) = Frecency::load(dir.path().join("frecency.json"));
        assert_eq!(warning, None);
        assert_eq!(f.score("ws:w1", NOW), 0.0);
    }

    #[test]
    fn record_increments_and_scores_by_age() {
        let dir = tempfile::tempdir().unwrap();
        let (mut f, _) = Frecency::load(dir.path().join("frecency.json"));
        f.record("ws:w1", NOW);
        f.record("ws:w1", NOW);
        assert_eq!(f.score("ws:w1", NOW), 8.0);
        assert_eq!(f.score("ws:w1", NOW + DAY), 1.0);
        assert_eq!(f.score("ws:other", NOW), 0.0);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("frecency.json");
        let (mut f, _) = Frecency::load(path.clone());
        f.record("tab:w1:t1", NOW);
        f.save(NOW).unwrap();
        let (loaded, warning) = Frecency::load(path.clone());
        assert_eq!(warning, None);
        assert_eq!(loaded.score("tab:w1:t1", NOW), 4.0);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn save_prunes_entries_older_than_90_days() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frecency.json");
        let (mut f, _) = Frecency::load(path.clone());
        f.record("old", NOW - 91 * DAY);
        f.record("fresh", NOW - DAY);
        f.save(NOW).unwrap();
        let (loaded, _) = Frecency::load(path);
        assert_eq!(loaded.score("old", NOW), 0.0);
        assert!(loaded.score("fresh", NOW) > 0.0);
    }

    #[test]
    fn corrupt_file_is_empty_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frecency.json");
        std::fs::write(&path, "{not json").unwrap();
        let (f, warning) = Frecency::load(path);
        assert!(warning.unwrap().contains("corrupt"));
        assert_eq!(f.score("anything", NOW), 0.0);
    }
}
