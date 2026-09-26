//! Finds the package.json that owns a directory, reads its scripts in file
//! order, and works out which package manager runs them.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, de::IgnoredAny};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl Manager {
    pub fn as_str(self) -> &'static str {
        match self {
            Manager::Npm => "npm",
            Manager::Pnpm => "pnpm",
            Manager::Yarn => "yarn",
            Manager::Bun => "bun",
        }
    }

    /// The shell line that runs `script`, e.g. `pnpm run 'build:prod'`.
    pub fn run_line(self, script: &str) -> String {
        format!("{} run {}", self.as_str(), shell_quote(script))
    }

    /// `packageManager` is corepack's `name@version` field.
    fn from_package_manager(field: &str) -> Option<Self> {
        match field.split('@').next()? {
            "npm" => Some(Manager::Npm),
            "pnpm" => Some(Manager::Pnpm),
            "yarn" => Some(Manager::Yarn),
            "bun" => Some(Manager::Bun),
            _ => None,
        }
    }

    fn from_lockfile(dir: &Path) -> Option<Self> {
        const LOCKFILES: [(&str, Manager); 5] = [
            ("pnpm-lock.yaml", Manager::Pnpm),
            ("yarn.lock", Manager::Yarn),
            ("bun.lock", Manager::Bun),
            ("bun.lockb", Manager::Bun),
            ("package-lock.json", Manager::Npm),
        ];
        LOCKFILES.iter().find(|(file, _)| dir.join(file).is_file()).map(|&(_, manager)| manager)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub dir: PathBuf,
    pub name: String,
    pub manager: Manager,
    pub scripts: Vec<Script>,
}

impl Project {
    /// Walks up from `start` to the repository root (a directory holding
    /// `.git`) or the filesystem root. The nearest package.json with scripts
    /// wins, so a pane inside a script-less library of an nx monorepo still
    /// finds the root's helper scripts.
    pub fn discover(start: &Path) -> Result<Self, String> {
        let search = search_path(start);
        let mut fallback = None;
        for dir in &search {
            let path = dir.join("package.json");
            let Ok(raw) = std::fs::read_to_string(&path) else { continue };
            let manifest: Manifest = serde_json::from_str(&raw).map_err(|err| format!("{}: {err}", path.display()))?;
            if !manifest.scripts.0.is_empty() {
                return Ok(Self::new(dir, manifest, &search));
            }
            fallback.get_or_insert((*dir, manifest));
        }
        match fallback {
            Some((dir, manifest)) => Ok(Self::new(dir, manifest, &search)),
            None => Err(format!("no package.json in {} or its parents", start.display())),
        }
    }

    fn new(dir: &Path, manifest: Manifest, search: &[&Path]) -> Self {
        let manager = manifest
            .package_manager
            .as_deref()
            .and_then(Manager::from_package_manager)
            .or_else(|| search.iter().skip_while(|d| **d != dir).find_map(|d| Manager::from_lockfile(d)))
            .unwrap_or(Manager::Npm);
        let name = manifest
            .name
            .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| dir.display().to_string());
        Self { dir: dir.to_path_buf(), name, manager, scripts: manifest.scripts.0 }
    }
}

/// `start` and its ancestors, ending at the first one that holds `.git`.
fn search_path(start: &Path) -> Vec<&Path> {
    let mut dirs = Vec::new();
    for dir in start.ancestors() {
        dirs.push(dir);
        if dir.join(".git").exists() {
            break;
        }
    }
    dirs
}

/// Script names are usually plain (`build`, `start:api`); anything else is
/// single-quoted so the shell passes it through untouched.
fn shell_quote(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_:./@+=,%".contains(c);
    if !value.is_empty() && value.chars().all(plain) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[derive(Deserialize)]
struct Manifest {
    name: Option<String>,
    #[serde(rename = "packageManager")]
    package_manager: Option<String>,
    #[serde(default)]
    scripts: Scripts,
}

/// Scripts in file order: authors group related scripts, and a map would sort
/// them. Non-string values are skipped rather than failing the whole file.
#[derive(Default)]
struct Scripts(Vec<Script>);

impl<'de> Deserialize<'de> for Scripts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ScriptsVisitor;

        impl<'de> Visitor<'de> for ScriptsVisitor {
            type Value = Scripts;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object of script names to commands")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Scripts, A::Error> {
                let mut scripts = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(name) = map.next_key::<String>()? {
                    match map.next_value::<StringOrOther>()? {
                        StringOrOther::String(command) => scripts.push(Script { name, command }),
                        StringOrOther::Other(IgnoredAny) => {}
                    }
                }
                Ok(Scripts(scripts))
            }
        }

        deserializer.deserialize_map(ScriptsVisitor)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrOther {
    String(String),
    Other(IgnoredAny),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, file: &str, content: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(file), content).unwrap();
    }

    fn names(project: &Project) -> Vec<&str> {
        project.scripts.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn scripts_keep_file_order_and_skip_non_strings() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"name":"mono","scripts":{"start:api":"nx serve api","build":"nx run-many -t build","bad":1,"lint":"nx lint"}}"#,
        );
        let project = Project::discover(dir.path()).unwrap();
        assert_eq!(project.name, "mono");
        assert_eq!(names(&project), ["start:api", "build", "lint"]);
        assert_eq!(project.scripts[0].command, "nx serve api");
        assert_eq!(project.manager, Manager::Npm);
    }

    #[test]
    fn nearest_package_with_scripts_wins_over_empty_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        write(dir.path(), "package.json", r#"{"name":"root","scripts":{"dev":"nx serve web"}}"#);
        write(dir.path(), "pnpm-lock.yaml", "");
        let lib = dir.path().join("libs/ui");
        write(&lib, "package.json", r#"{"name":"@mono/ui"}"#);
        let project = Project::discover(&lib).unwrap();
        assert_eq!(project.name, "root");
        assert_eq!(project.dir, dir.path());
        assert_eq!(project.manager, Manager::Pnpm);
    }

    #[test]
    fn nested_package_with_scripts_wins_and_uses_root_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "yarn.lock", "");
        write(dir.path(), "package.json", r#"{"scripts":{"dev":"a"}}"#);
        let app = dir.path().join("apps/web");
        write(&app, "package.json", r#"{"scripts":{"serve":"b"}}"#);
        let project = Project::discover(&app).unwrap();
        assert_eq!(names(&project), ["serve"]);
        assert_eq!(project.name, "web");
        assert_eq!(project.manager, Manager::Yarn);
    }

    #[test]
    fn package_manager_field_beats_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package-lock.json", "{}");
        write(dir.path(), "package.json", r#"{"packageManager":"bun@1.1.0","scripts":{"x":"y"}}"#);
        assert_eq!(Project::discover(dir.path()).unwrap().manager, Manager::Bun);
    }

    #[test]
    fn search_stops_at_the_repo_root() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package.json", r#"{"scripts":{"outside":"x"}}"#);
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let err = Project::discover(&repo).unwrap_err();
        assert!(err.starts_with("no package.json in"), "{err}");
    }

    #[test]
    fn only_empty_package_is_returned_without_scripts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        write(dir.path(), "package.json", r#"{"name":"bare"}"#);
        let project = Project::discover(dir.path()).unwrap();
        assert_eq!(project.name, "bare");
        assert!(project.scripts.is_empty());
    }

    #[test]
    fn invalid_json_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package.json", "{nope");
        let err = Project::discover(dir.path()).unwrap_err();
        assert!(err.contains("package.json"), "{err}");
    }

    #[test]
    fn run_line_quotes_only_when_needed() {
        assert_eq!(Manager::Pnpm.run_line("start:api"), "pnpm run start:api");
        assert_eq!(Manager::Npm.run_line("build prod"), "npm run 'build prod'");
        assert_eq!(Manager::Yarn.run_line("it's"), r"yarn run 'it'\''s'");
    }
}
