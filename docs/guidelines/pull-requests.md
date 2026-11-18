# Pull requests and CI workflow changes

All changes — including those by AI agents and maintainers — land through a pull request. Never merge a branch directly or push to `main`.

1. Work on a feature branch (`wave2-mixer`, `docs/...`, `fix/...`) cut from `main`.
2. Open a PR into `main` with `gh pr create --base main`. `rust.yml` (the Rust CI: fmt, clippy, test on Linux, the `web` job (front-end build, `npm test`, `npm run check` and the `gui-tauri` window tests under Xvfb), `cargo deny`, the `onnx` job, the ASIO SDK guard) runs on every pull request; its 10-minute `soak` job runs only on the weekly schedule and by hand. `package.yml` (installers on Linux, Windows and macOS through `tools/package.sh`) runs on a pull request only when the packaging changes, and on a `v*` tag or by hand. The description must include:
   - **Summary:** what changed and why (link the issue).
   - **Changes:** key crates/modules added or modified.
   - **Testing:** commands run and results (`cargo test --workspace`, manual checks).
   - **Follow-ups / known gaps:** anything left undone.
   - **Extension points:** whether it adds, changes or could block one ([extensibility.md](extensibility.md)).
3. Docs match the repo before the PR opens: check `README.md` (status, screenshots, build notes), and any doc the change touches against the current code, and fix stale claims in the same PR. Outdated docs send people to rebuild or fix what already exists or is in flight.
4. Merge the PR (`gh pr merge --merge`) once the build and tests pass, so history stays traceable to a PR.
5. Stack dependent work: when a PR builds on or touches the same files as an open PR, branch from that PR's branch and set `--base <that-branch>`. Keep the stack current by merging the parent branch into the child (merge commits, never force-push), so merging in order needs no rebase. After the parent merges, retarget the child with `gh pr edit --base main`. Use plain merge commits, not squash, so stacked history stays intact.

## CI Workflow Changes (required)

Any change to `.github/workflows/` must be tested locally with [`act`](https://github.com/nektos/act) before opening the PR.

1. Run the affected job(s): `act -j <job> [-W .github/workflows/<file>.yml]` (add `--container-architecture linux/amd64` if needed). List jobs with `act -l`.
2. Fix failures locally; don't push to find out.
3. Put the `act` command and result in the PR's Testing section. Note anything `act` can't cover (e.g. macOS/Windows runners, secrets).
