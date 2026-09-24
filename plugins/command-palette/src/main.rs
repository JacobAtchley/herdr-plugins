mod frecency;
mod item;
mod matcher;
mod sources;

#[cfg(test)]
mod testing;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
