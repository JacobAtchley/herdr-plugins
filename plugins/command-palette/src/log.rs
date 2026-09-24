//! Diagnostics go to a file: the palette owns the terminal while it runs.

use std::io::Write;
use std::path::Path;

use crate::frecency::now_unix;

/// Best effort: logging must never break the palette.
pub fn append(dir: &Path, message: &str) {
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("palette.log"))?;
        writeln!(file, "{} {message}", now_unix())
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
        let log = std::fs::read_to_string(state.join("palette.log")).unwrap();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" first"));
        assert!(lines[1].ends_with(" second"));
    }
}
