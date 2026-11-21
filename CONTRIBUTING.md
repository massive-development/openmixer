# Contributing to OpenMixer DJ

Thank you for your interest in OpenMixer. This page covers what every contribution needs. The detailed rules live in `docs/guidelines/` and are linked below.

## Getting started

1. Set up the toolchain and system packages from the Building section of [README.md](README.md).
2. Fork the repository and create a branch from `main` (`fix/...`, `feat/...`, `docs/...`).
3. Read [docs/PRODUCT.md](docs/PRODUCT.md) if your change touches anything a DJ sees or reaches, and check [docs/adr/](docs/adr/INDEX.md) for decisions that apply.

## Clean-room rule

OpenMixer is a clean-room implementation ([ADR 0001](docs/adr/0001-clean-room-doctrine.md)). Every contribution must follow it.

- Implement features independently. Workflows and common layout conventions are free to use.
- Do not copy names, logos, artwork, icons, manual text, presets or pixel layouts from other DJ programs.
- Do not copy Mixxx application code.
- Do not bundle machine-learning model weights; they are downloaded on first use.
- Record the source and licence of every bundled asset in [docs/legal/asset-provenance.csv](docs/legal/asset-provenance.csv).

## Sign-off

OpenMixer is licensed under AGPLv3 with no contributor licence agreement ([ADR 0003](docs/adr/0003-pure-agpl-no-cla.md)). Every commit needs a [Developer Certificate of Origin](https://developercertificate.org/) sign-off:

```bash
git commit -s -m "fix: describe the change"
```

## Code standards

The full rules are in [docs/guidelines/rust.md](docs/guidelines/rust.md). In short:

- **Real-time safety.** Code on the audio thread does not allocate, lock, block or perform I/O. It talks to the rest of the app through `rtrb` queues and atomics.
- **Unsafe code** is allowed only in per-module exceptions, each `unsafe` block carries a `// SAFETY:` comment, and any new FFI or plugin load path needs a leak check.
- **No warnings.** `cargo clippy --workspace --all-targets -- -D warnings` must pass. An `#[allow]` needs its reason written next to it.
- **Errors** use `thiserror` in library crates. Avoid `unwrap()` and `expect()` outside tests.
- **Comments** are short and say why, not what. A function that needs a long comment should be split.
- **Modules and crates** have one focused responsibility. Dependencies point toward `om-core`, and library crates never depend on the UI or the binary.
- **Patterns.** Reuse the patterns in [docs/DESIGN_PATTERNS.md](docs/DESIGN_PATTERNS.md), and add an entry when you introduce a new one.
- **Extensibility.** Don't hard-code what a plugin could reasonably replace, and don't block users from loading their own plugins. See [docs/guidelines/extensibility.md](docs/guidelines/extensibility.md).
- **Style.** Standard Rust naming, and `cargo fmt --all` before committing.

## Tests

Put unit tests in a `#[cfg(test)]` module next to the code and integration tests in the crate's `tests/` directory. Use the shared helpers in `crates/om-testing` (fixtures, signals, temporary directories, bounded waits) rather than writing new ones. Tests must run in parallel and must not sleep to synchronise; long timing tests are `#[ignore]`d and run in the weekly CI job.

Before opening a pull request, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

More commands (soak and UI gates, benchmarks, optional features) are in [docs/guidelines/build.md](docs/guidelines/build.md).

## Pull requests

Every change lands through a pull request into `main`. The full process, including stacked PRs and CI workflow changes, is in [docs/guidelines/pull-requests.md](docs/guidelines/pull-requests.md). A pull request should:

- Explain what changed and why, and link the issue.
- List the commands you ran and their results.
- Include tests for new behaviour.
- Say whether it adds, changes or could block a plugin extension point.
- Include a screenshot you have looked at if it changes what the window shows, and use real music for anything involving waveforms, the library or analysis. See [docs/guidelines/experience-bar.md](docs/guidelines/experience-bar.md).
- Update or add an ADR in `docs/adr/` when it changes an architectural decision.
- Show that it was tested locally with [`act`](https://github.com/nektos/act) if it changes `.github/workflows/`.

## Questions

Open an [issue](https://github.com/massive-development/openmixer/issues/new/choose) for bugs and feature requests. Report security problems privately as described in [SECURITY.md](SECURITY.md). For anything else, write to contact@openmixer.dev.
