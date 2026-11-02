//! Headless commands for other front ends (the Herdr app's mod): finds the
//! project at `--cwd`, ranks its scripts with the TUI's matcher and prints
//! one JSON object.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};
use unicode_segmentation::UnicodeSegmentation;

use crate::matcher::ScriptMatcher;
use crate::project::Project;

/// Prints the result of one command; exits 1 when the request failed and 2
/// when the arguments made no sense.
pub fn main(args: &[String]) -> ExitCode {
    let (value, code) = match parse(args) {
        Ok((cwd, query)) => {
            let value = scripts(&cwd, query.as_deref().unwrap_or(""));
            let code = if value["ok"] == true { 0 } else { 1 };
            (value, code)
        }
        Err(error) => (json!({ "ok": false, "error": error }), 2),
    };
    println!("{value}");
    ExitCode::from(code)
}

/// Options may come anywhere; what's left is the command.
pub fn parse(args: &[String]) -> Result<(PathBuf, Option<String>), String> {
    let (mut cwd, mut query) = (None, None);
    let mut words = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--cwd" => cwd = Some(PathBuf::from(value()?)),
            "--query" => query = Some(value()?),
            _ => words.push(arg.as_str()),
        }
    }
    let cwd = cwd.ok_or("missing --cwd <dir>")?;
    match words.as_slice() {
        ["scripts"] => Ok((cwd, query)),
        other => Err(format!("unknown command: {}", other.join(" "))),
    }
}

/// The project that owns `cwd` and its scripts ranked for `query`.
pub fn scripts(cwd: &Path, query: &str) -> Value {
    let project = match Project::discover(cwd) {
        Ok(project) => project,
        Err(error) => return json!({ "ok": false, "error": error }),
    };
    let shell = std::env::var("SHELL").unwrap_or_default();
    let ranked = ScriptMatcher::default().rank(query, &project.scripts);
    let scripts: Vec<Value> = ranked
        .iter()
        .map(|r| {
            let script = &project.scripts[r.index];
            json!({
                "index": r.index,
                "name": script.name,
                "command": script.command,
                "segments": segments(&script.name, &r.highlights),
                "argv": project.manager.argv(&script.name),
                "split_argv": project.manager.shell_argv(&script.name, &shell),
            })
        })
        .collect();
    json!({
        "ok": true,
        "project": {
            "dir": project.dir.to_string_lossy(),
            "name": project.name,
            "manager": project.manager.as_str(),
        },
        "total": project.scripts.len(),
        "scripts": scripts,
    })
}

/// `name` cut into runs of graphemes that are all highlighted or all not, so
/// the mod draws them without knowing what a grapheme is.
fn segments(name: &str, highlights: &[usize]) -> Vec<Value> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for (i, g) in name.graphemes(true).enumerate() {
        let hit = highlights.contains(&i);
        match runs.last_mut() {
            Some((text, last)) if *last == hit => text.push_str(g),
            _ => runs.push((g.to_string(), hit)),
        }
    }
    runs.into_iter().map(|(text, hit)| json!({ "text": text, "hit": hit })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn project(scripts: &str, lockfile: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), format!(r#"{{"name": "mono", "scripts": {scripts}}}"#))
            .unwrap();
        if let Some(lock) = lockfile {
            std::fs::write(dir.path().join(lock), "").unwrap();
        }
        dir
    }

    fn mono() -> tempfile::TempDir {
        project(
            r#"{"start:web": "nx serve web", "start:api": "nx serve api", "lint": "nx run-many -t lint"}"#,
            Some("pnpm-lock.yaml"),
        )
    }

    fn names(value: &Value) -> Vec<&str> {
        value["scripts"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect()
    }

    #[test]
    fn parse_needs_cwd_and_knows_scripts() {
        assert_eq!(parse(&args(&["scripts", "--cwd", "/r"])), Ok((PathBuf::from("/r"), None)));
        assert_eq!(
            parse(&args(&["scripts", "--query", "x", "--cwd", "/r"])),
            Ok((PathBuf::from("/r"), Some("x".into())))
        );
        assert!(parse(&args(&["scripts"])).is_err());
        assert!(parse(&args(&["run", "--cwd", "/r"])).is_err());
        assert!(parse(&args(&["scripts", "--cwd", "/r", "--query"])).is_err());
    }

    #[test]
    fn lists_scripts_in_file_order() {
        let dir = mono();
        let value = scripts(dir.path(), "");
        assert_eq!(value["ok"], true, "{value}");
        assert_eq!(value["project"]["manager"], "pnpm");
        assert_eq!(value["project"]["name"], "mono");
        assert_eq!(value["project"]["dir"], dir.path().to_str().unwrap());
        assert_eq!(value["total"], 3);
        assert_eq!(names(&value), ["start:web", "start:api", "lint"]);
        for script in value["scripts"].as_array().unwrap() {
            assert_eq!(script["argv"], json!(["pnpm", "run", script["name"]]));
            assert_eq!(
                script["split_argv"].as_array().unwrap()[4..],
                json!(["pnpm", "run", script["name"]]).as_array().unwrap()[..]
            );
        }
        assert_eq!(value["scripts"][1]["command"], "nx serve api");
        assert_eq!(value["scripts"][1]["index"], 1);
    }

    #[test]
    fn query_ranks_and_highlights() {
        let dir = mono();
        assert_eq!(names(&scripts(dir.path(), "serve api"))[0], "start:api");
        let dir = project(r#"{"dev": "vite", "build": "vite build"}"#, None);
        let value = scripts(dir.path(), "dev");
        assert_eq!(value["scripts"][0]["name"], "dev");
        assert_eq!(value["scripts"][0]["segments"], json!([{"text": "dev", "hit": true}]));
    }

    #[test]
    fn query_uses_tui_matcher_syntax() {
        let dir = mono();
        let value = scripts(dir.path(), "!lint");
        assert!(!names(&value).contains(&"lint"), "{value}");
        let project = Project::discover(dir.path()).unwrap();
        for query in ["!lint", "^start", "'api", "web$", "sv"] {
            let want: Vec<&str> = ScriptMatcher::default()
                .rank(query, &project.scripts)
                .iter()
                .map(|r| project.scripts[r.index].name.as_str())
                .collect();
            assert_eq!(names(&scripts(dir.path(), query)), want, "{query}");
        }
    }

    #[test]
    fn no_matches_keeps_total() {
        let value = scripts(mono().path(), "zzz");
        assert_eq!(value["scripts"], json!([]));
        assert_eq!(value["total"], 3);
    }

    #[test]
    fn segments_split_on_graphemes() {
        let dir = project(r#"{"tëst": "x"}"#, None);
        let value = scripts(dir.path(), "ts");
        assert_eq!(
            value["scripts"][0]["segments"],
            json!([
                {"text": "t", "hit": true},
                {"text": "ë", "hit": false},
                {"text": "s", "hit": true},
                {"text": "t", "hit": false},
            ])
        );
        assert_eq!(scripts(dir.path(), "")["scripts"][0]["segments"], json!([{"text": "tëst", "hit": false}]));
    }

    #[test]
    fn argv_keeps_odd_names_whole() {
        let dir = project(r#"{"it's build": "x"}"#, None);
        assert_eq!(scripts(dir.path(), "")["scripts"][0]["argv"], json!(["npm", "run", "it's build"]));
    }

    #[test]
    fn missing_package_json_is_an_error() {
        let empty = tempfile::tempdir().unwrap();
        std::fs::create_dir(empty.path().join(".git")).unwrap();
        let value = scripts(empty.path(), "");
        assert_eq!(value["ok"], false);
        assert!(value["error"].as_str().unwrap().starts_with("no package.json in"), "{value}");

        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join("package.json"), "{ nope").unwrap();
        assert_eq!(scripts(broken.path(), "")["ok"], false);
    }
}
