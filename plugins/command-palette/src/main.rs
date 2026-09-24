use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
