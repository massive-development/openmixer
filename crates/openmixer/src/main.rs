#![forbid(unsafe_code)]

use std::process::ExitCode;

#[cfg(feature = "gui")]
mod gui;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(feature = "gui")]
    if args
        .first()
        .is_some_and(|a| a.starts_with("--") && a != "--version")
        || args.is_empty()
    {
        return gui::main(&args);
    }
    match args.first().map(String::as_str) {
        Some("--smoke") => {
            println!("openmixer {} smoke ok", om_core::VERSION);
            ExitCode::SUCCESS
        }
        Some("--version") => {
            println!("openmixer {}", om_core::VERSION);
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: openmixer --smoke | --version");
            ExitCode::FAILURE
        }
    }
}
