# Project Structure

```
openmixer/
├── .github/workflows/        # rust.yml (the Rust CI and the web front end job, every PR; weekly soak), package.yml (installers per OS) and issue-heatmap.yml
├── .github/ISSUE_TEMPLATE/    # bug and feature forms; pull_request_template.md beside it
├── crates/                   # Cargo workspace members (om-core ... om-stems, om-plugins, om-ui, openmixer)
│   ├── om-ui-gpui/           # GPUI window (the openmixer `gui` feature, on by default)
│   ├── om-ui-bridge/         # ADR 0009: snapshot packet, peaks, theme CSS, control specs and dispatch for a web front end
│   └── om-ui-tauri/          # ADR 0009 web front end: Tauri v2 window (the `gui-tauri` feature, off by default); web/ Svelte front end, icons/
├── docs/
│   ├── adr/                  # Architecture Decision Records
│   ├── controllers/          # Controller map notes
│   ├── features/             # Feature notes: automix, mapping editor, recording, sampler, stems, web front end
│   ├── releases/             # Release notes, one file per tag (v<version>.md), read by package.yml
│   ├── guidelines/           # Build, control ids, experience bar, extensibility, pull requests, Rust
│   ├── legal/                # Legal documentation and the asset provenance record
│   ├── screenshots/          # Staged window screenshots and REVIEW.md
│   └── testing/              # Hardware checklist (after the platform refresh)
├── examples/plugins/         # Example plugins: gain-plugin, gain-plugin-cdylib, param-plugin-cdylib, clap-gain-fixture
├── fixtures/                 # Small test inputs: controller maps and traces, library exports; ui-bridge/ golden packet, meter, fader law and knob text tables
├── resources/                # Built-in theme JSON, generic controller maps, bundled fonts
├── tools/                    # package.sh (+ package/: Dockerfile, checksums.sh, SignPath config), issue_heatmap.py, screenshots.sh, ui-bench.sh, ui-drive.py, export_htdemucs.py, htdemucs_reference.py, clap_leak_check.sh
├── Cargo.toml                # Workspace manifest
├── Cargo.lock
├── rust-toolchain.toml       # Pinned toolchain
├── rustfmt.toml
├── clippy.toml
└── deny.toml                 # Licence and ban policy (cargo deny), including what the GPUI tree needs
```
