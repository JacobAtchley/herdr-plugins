//! Parses `git status --porcelain=v2 --branch --show-stash -z`.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub branch: BranchInfo,
    pub stashes: usize,
    /// Ordered by section, then path (git's order within a section).
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchInfo {
    /// `None` when HEAD is detached.
    pub head: Option<String>,
    /// Abbreviated commit; `None` before the first commit.
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    Conflict,
    Staged,
    Unstaged,
    Untracked,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Conflict => "Conflicts",
            Section::Staged => "Staged",
            Section::Unstaged => "Changes",
            Section::Untracked => "Untracked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub section: Section,
    /// Status letter for the section: `M`, `A`, `D`, `R`, `C`, `T`, `U` or `?`.
    pub code: char,
    pub path: String,
    /// Source path of a rename or copy.
    pub orig: Option<String>,
}

impl Snapshot {
    pub fn has_staged(&self) -> bool {
        self.entries.iter().any(|e| e.section == Section::Staged)
    }

    pub fn is_unborn(&self) -> bool {
        self.branch.oid.is_none()
    }
}

const SHORT_OID: usize = 7;

pub fn parse(raw: &[u8]) -> Result<Snapshot, String> {
    let mut snapshot = Snapshot::default();
    let mut records = raw.split(|&b| b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let line = String::from_utf8_lossy(record);
        let bad = || format!("unexpected git status line: {line:?}");
        let (kind, rest) = line.split_once(' ').ok_or_else(bad)?;
        match kind {
            "#" => header(&mut snapshot, rest),
            "1" => {
                let fields = splitn::<8>(rest).ok_or_else(bad)?;
                push_changed(&mut snapshot.entries, fields[0], fields[7], None);
            }
            "2" => {
                let fields = splitn::<9>(rest).ok_or_else(bad)?;
                let orig = records.next().map(|r| String::from_utf8_lossy(r).into_owned()).ok_or_else(bad)?;
                push_changed(&mut snapshot.entries, fields[0], fields[8], Some(orig));
            }
            "u" => {
                let fields = splitn::<10>(rest).ok_or_else(bad)?;
                snapshot.entries.push(Entry {
                    section: Section::Conflict,
                    code: 'U',
                    path: fields[9].to_string(),
                    orig: None,
                });
            }
            "?" => snapshot.entries.push(Entry {
                section: Section::Untracked,
                code: '?',
                path: rest.to_string(),
                orig: None,
            }),
            // `!` (ignored) is never requested; skip anything newer git adds.
            _ => {}
        }
    }
    // Stable: git's path order is kept within each section.
    snapshot.entries.sort_by_key(|e| e.section);
    Ok(snapshot)
}

fn header(snapshot: &mut Snapshot, rest: &str) {
    let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
    let branch = &mut snapshot.branch;
    match key {
        "branch.oid" if value != "(initial)" => branch.oid = Some(value.chars().take(SHORT_OID).collect()),
        "branch.head" if value != "(detached)" => branch.head = Some(value.to_string()),
        "branch.upstream" => branch.upstream = Some(value.to_string()),
        "branch.ab" => {
            for part in value.split(' ') {
                if let Some(n) = part.strip_prefix('+') {
                    branch.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    branch.behind = n.parse().unwrap_or(0);
                }
            }
        }
        "stash" => snapshot.stashes = value.parse().unwrap_or(0),
        _ => {}
    }
}

/// An `XY` pair becomes up to two entries: `X` is the index (staged) side,
/// `Y` the worktree side; `.` means unchanged.
fn push_changed(entries: &mut Vec<Entry>, xy: &str, path: &str, orig: Option<String>) {
    let mut codes = xy.chars();
    let (x, y) = (codes.next().unwrap_or('.'), codes.next().unwrap_or('.'));
    if x != '.' {
        entries.push(Entry { section: Section::Staged, code: x, path: path.to_string(), orig: orig.clone() });
    }
    if y != '.' {
        entries.push(Entry { section: Section::Unstaged, code: y, path: path.to_string(), orig: None });
    }
}

/// Splits into exactly `N` fields; the last keeps any spaces (paths may contain them).
fn splitn<const N: usize>(s: &str) -> Option<[&str; N]> {
    let mut parts = s.splitn(N, ' ');
    let fields: [&str; N] = std::array::from_fn(|_| parts.next().unwrap_or(""));
    (!fields[N - 1].is_empty()).then_some(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(lines: &[&str]) -> Vec<u8> {
        lines.iter().flat_map(|l| l.bytes().chain([0])).collect()
    }

    fn entry(section: Section, code: char, path: &str) -> Entry {
        Entry { section, code, path: path.into(), orig: None }
    }

    #[test]
    fn parses_branch_headers() {
        let raw = z(&[
            "# branch.oid 0123456789abcdef0123456789abcdef01234567",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            "# stash 3",
        ]);
        let snapshot = parse(&raw).unwrap();
        assert_eq!(
            snapshot.branch,
            BranchInfo {
                head: Some("main".into()),
                oid: Some("0123456".into()),
                upstream: Some("origin/main".into()),
                ahead: 2,
                behind: 1,
            }
        );
        assert_eq!(snapshot.stashes, 3);
        assert!(snapshot.entries.is_empty());
        assert!(!snapshot.is_unborn());
    }

    #[test]
    fn initial_and_detached_heads_are_none() {
        let snapshot = parse(&z(&["# branch.oid (initial)", "# branch.head (detached)"])).unwrap();
        assert_eq!(snapshot.branch.head, None);
        assert!(snapshot.is_unborn());
    }

    #[test]
    fn splits_entries_into_sections_in_order() {
        let raw = z(&[
            "# branch.head main",
            "? new file.txt",
            "1 .M N... 100644 100644 100644 aaa bbb README.md",
            "1 MM N... 100644 100644 100644 aaa bbb src/app.rs",
            "1 A. N... 000000 100644 100644 000 bbb src/with space.rs",
            "u UU N... 100644 100644 100644 100644 a b c conflicted.rs",
        ]);
        let snapshot = parse(&raw).unwrap();
        assert_eq!(
            snapshot.entries,
            [
                entry(Section::Conflict, 'U', "conflicted.rs"),
                entry(Section::Staged, 'M', "src/app.rs"),
                entry(Section::Staged, 'A', "src/with space.rs"),
                entry(Section::Unstaged, 'M', "README.md"),
                entry(Section::Unstaged, 'M', "src/app.rs"),
                entry(Section::Untracked, '?', "new file.txt"),
            ]
        );
        assert!(snapshot.has_staged());
    }

    #[test]
    fn rename_reads_the_original_path_from_the_next_record() {
        let raw = z(&["2 R. N... 100644 100644 100644 aaa aaa R100 new name.rs", "old name.rs", "? after.txt"]);
        let snapshot = parse(&raw).unwrap();
        assert_eq!(
            snapshot.entries,
            [
                Entry {
                    section: Section::Staged,
                    code: 'R',
                    path: "new name.rs".into(),
                    orig: Some("old name.rs".into())
                },
                entry(Section::Untracked, '?', "after.txt"),
            ]
        );
    }

    #[test]
    fn malformed_records_are_errors_and_unknown_kinds_are_skipped() {
        assert!(parse(&z(&["1 .M N..."])).is_err());
        assert!(parse(&z(&["2 R. N... 100644 100644 100644 aaa aaa R100 a.rs"])).is_err());
        assert!(parse(&z(&["garbage"])).is_err());
        assert_eq!(parse(&z(&["! ignored.log"])).unwrap(), Snapshot::default());
    }

    #[test]
    fn empty_output_is_an_empty_snapshot() {
        assert_eq!(parse(b"").unwrap(), Snapshot::default());
        assert!(!Snapshot::default().has_staged());
    }
}
