//! Writes a placeholder front end when `web/dist` has not been built, so the crate (and
//! `cargo check --workspace`) builds without Node; then runs `tauri_build`.

use std::path::Path;

const PLACEHOLDER: &str = "<!doctype html><title>OpenMixer</title>\n";
const INDEX: &str = "web/dist/index.html";

fn main() {
    // Reruns when the built front end appears or goes, so a deleted dist gets its placeholder back.
    println!("cargo:rerun-if-changed={INDEX}");
    if let Err(error) = write_placeholder(Path::new(INDEX)) {
        println!("cargo:warning=could not write the placeholder front end {INDEX}: {error}");
    }
    tauri_build::build();
}

fn write_placeholder(index: &Path) -> std::io::Result<()> {
    if index.exists() {
        return Ok(());
    }
    if let Some(dir) = index.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(index, PLACEHOLDER)
}
