mod app;
mod frecency;
mod exec;
mod item;
mod matcher;
mod render;
mod sources;

#[cfg(test)]
mod testing;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
