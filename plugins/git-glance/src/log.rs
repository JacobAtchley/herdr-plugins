//! Diagnostics go to a file: the glance owns the terminal while it runs.

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Best effort: logging must never break the glance.
pub fn append(dir: &Path, message: &str) {
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("glance.log"))?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        writeln!(file, "{now} {message}")
    };
    let _ = write();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_timestamped_lines_creating_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        append(&state, "first");
        append(&state, "second");
        let log = std::fs::read_to_string(state.join("glance.log")).unwrap();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" first"));
        assert!(lines[1].ends_with(" second"));
    }
}
