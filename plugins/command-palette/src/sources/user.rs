//! User-defined commands from `$HERDR_PLUGIN_CONFIG_DIR/commands.toml`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserCommand {
    pub title: String,
    /// Shell script run with `sh -c`.
    #[serde(default)]
    pub run: Option<String>,
    /// Program and arguments run directly, without a shell.
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandsFile {
    #[serde(default)]
    commands: Vec<UserCommand>,
}

pub fn parse(src: &str) -> Result<Vec<UserCommand>, String> {
    let file: CommandsFile = toml::from_str(src).map_err(|err| {
        let line = err.span().map(|span| src[..span.start].matches('\n').count() + 1);
        match line {
            Some(line) => format!("commands.toml line {line}: {}", err.message()),
            None => format!("commands.toml: {}", err.message()),
        }
    })?;
    for (index, cmd) in file.commands.iter().enumerate() {
        let which = format!("command {} (\"{}\")", index + 1, cmd.title);
        match (&cmd.run, &cmd.argv) {
            (Some(_), None) => {}
            (None, Some(argv)) if !argv.is_empty() => {}
            (None, Some(_)) => return Err(format!("commands.toml: {which} has an empty argv")),
            _ => return Err(format!("commands.toml: {which} needs exactly one of `run` or `argv`")),
        }
    }
    Ok(file.commands)
}

pub fn load(path: &Path) -> Result<Vec<UserCommand>, String> {
    match std::fs::read_to_string(path) {
        Ok(src) => parse(&src),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("commands.toml: {err}")),
    }
}

impl UserCommand {
    /// Builds the process to spawn. `default_cwd` is the focused pane's cwd.
    pub fn command(&self, default_cwd: Option<&str>) -> Command {
        let mut cmd = match (&self.run, &self.argv) {
            (Some(script), _) => {
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(script);
                cmd
            }
            (None, Some(argv)) => {
                let mut cmd = Command::new(&argv[0]);
                cmd.args(&argv[1..]);
                cmd
            }
            (None, None) => unreachable!("parse() rejects commands without run or argv"),
        };
        if let Some(dir) = self.cwd.as_deref().or(default_cwd) {
            cmd.current_dir(expand_home(dir));
        }
        cmd
    }
}

pub fn items(commands: &[UserCommand]) -> Vec<crate::item::Item> {
    use crate::item::{Action, Item, Kind};
    commands
        .iter()
        .map(|cmd| {
            Item::new(Kind::User, format!("user:{}", cmd.title), &cmd.title, Action::RunUser(cmd.clone()))
                .keywords(cmd.keywords.clone())
        })
        .collect()
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn parses_run_and_argv_commands() {
        let cmds = parse(
            r#"
            [[commands]]
            title = "Deploy staging"
            run = "just deploy staging"
            keywords = ["ship"]

            [[commands]]
            title = "Open notes"
            argv = ["open", "-a", "Notes"]
            cwd = "/tmp"
            "#,
        )
        .unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].run.as_deref(), Some("just deploy staging"));
        assert_eq!(cmds[0].keywords, ["ship"]);
        assert_eq!(cmds[1].argv.as_ref().unwrap(), &["open", "-a", "Notes"]);
        assert_eq!(cmds[1].cwd.as_deref(), Some("/tmp"));
    }

    #[test]
    fn empty_file_has_no_commands() {
        assert_eq!(parse("").unwrap(), vec![]);
    }

    #[test]
    fn requires_exactly_one_of_run_or_argv() {
        let both = parse("[[commands]]\ntitle = \"x\"\nrun = \"a\"\nargv = [\"b\"]\n").unwrap_err();
        assert_eq!(both, "commands.toml: command 1 (\"x\") needs exactly one of `run` or `argv`");
        let neither = parse("[[commands]]\ntitle = \"y\"\n").unwrap_err();
        assert!(neither.contains("command 1 (\"y\")"), "{neither}");
        let empty = parse("[[commands]]\ntitle = \"z\"\nargv = []\n").unwrap_err();
        assert!(empty.contains("empty argv"), "{empty}");
    }

    #[test]
    fn syntax_errors_report_the_line_on_one_line() {
        let err = parse("[[commands]]\ntitle =\n").unwrap_err();
        assert!(err.starts_with("commands.toml line 2: "), "{err}");
        assert!(!err.contains('\n'), "{err}");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = parse("[[commands]]\ntitle = \"x\"\nrun = \"a\"\ncommand = \"typo\"\n").unwrap_err();
        assert!(err.starts_with("commands.toml"), "{err}");
    }

    #[test]
    fn missing_file_means_no_commands() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("commands.toml")).unwrap(), vec![]);
    }

    #[test]
    fn load_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("commands.toml");
        std::fs::write(&path, "[[commands]]\ntitle = \"x\"\nrun = \"true\"\n").unwrap();
        assert_eq!(load(&path).unwrap()[0].title, "x");
    }

    #[test]
    fn run_builds_sh_command_with_default_cwd() {
        let cmd = UserCommand {
            title: "t".into(),
            run: Some("echo hi".into()),
            argv: None,
            keywords: vec![],
            cwd: None,
        }
        .command(Some("/repo"));
        assert_eq!(cmd.get_program(), "sh");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [OsStr::new("-c"), OsStr::new("echo hi")]);
        assert_eq!(cmd.get_current_dir().unwrap(), std::path::Path::new("/repo"));
    }

    #[test]
    fn argv_builds_direct_command_and_explicit_cwd_wins() {
        let cmd = UserCommand {
            title: "t".into(),
            run: None,
            argv: Some(vec!["ls".into(), "-la".into()]),
            keywords: vec![],
            cwd: Some("/etc".into()),
        }
        .command(Some("/repo"));
        assert_eq!(cmd.get_program(), "ls");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [OsStr::new("-la")]);
        assert_eq!(cmd.get_current_dir().unwrap(), std::path::Path::new("/etc"));
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_home("~"), std::path::PathBuf::from(&home));
        assert_eq!(expand_home("~/src"), std::path::PathBuf::from(&home).join("src"));
        assert_eq!(expand_home("/abs"), std::path::PathBuf::from("/abs"));
        assert_eq!(expand_home("~other"), std::path::PathBuf::from("~other"));
    }

    #[test]
    fn items_use_title_as_id_and_keep_keywords() {
        let cmds = parse("[[commands]]\ntitle = \"Deploy\"\nrun = \"x\"\nkeywords = [\"ship\"]\n").unwrap();
        let items = items(&cmds);
        assert_eq!(items[0].kind, crate::item::Kind::User);
        assert_eq!(items[0].id, "user:Deploy");
        assert_eq!(items[0].keywords, ["ship"]);
        assert_eq!(items[0].action, crate::item::Action::RunUser(cmds[0].clone()));
    }
}
