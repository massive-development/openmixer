#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
