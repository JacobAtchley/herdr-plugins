mod frecency;
mod item;
mod matcher;
mod sources;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
