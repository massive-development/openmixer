# Project Structure

```
openmixer/
├── .github/workflows/        # rust.yml (the Rust CI, every PR; weekly soak) and issue-heatmap.yml
├── crates/                   # Cargo workspace members (om-core ... om-stems, om-plugins, om-ui, openmixer)
│   ├── om-ui-gpui/           # GPUI window (the openmixer `gui` feature, on by default)
│   ├── om-ui-bridge/         # Toolkit-free wire formats for a web front end (ADR 0009)
│   └── om-ui-tauri/          # Trial Tauri v2 window; Svelte front end in web/ (the `gui-tauri` feature, off by default)
├── docs/
│   ├── adr/                  # Architecture Decision Records
│   ├── controllers/          # Controller map notes
│   ├── legal/                # Legal documentation and the asset provenance record
│   ├── screenshots/          # Staged window screenshots and REVIEW.md
│   └── testing/              # Hardware checklist (after the platform refresh)
├── examples/plugins/         # Example plugins: gain-plugin, gain-plugin-cdylib, param-plugin-cdylib, clap-gain-fixture
├── fixtures/                 # Small test inputs: controller maps and traces, library exports
├── resources/                # Built-in theme JSON, generic controller maps, bundled fonts
├── tools/                    # issue_heatmap.py, screenshots.sh, export_htdemucs.py, htdemucs_reference.py, clap_leak_check.sh
├── Cargo.toml                # Workspace manifest
├── Cargo.lock
├── rust-toolchain.toml       # Pinned toolchain
├── rustfmt.toml
├── clippy.toml
└── deny.toml                 # Licence and ban policy (cargo deny), including what the GPUI tree needs
```
