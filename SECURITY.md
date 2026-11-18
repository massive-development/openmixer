# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub: on the repository's **Security** tab, choose **Report a vulnerability**. Do not open a public issue for them.

Useful things to include: the version (`openmixer --version`) or commit, the operating system, what an attacker can do, and the steps or file that trigger it.

You will get an answer within a week. Once a fix is released, the advisory is published with credit to the reporter, unless you would rather not be named.

## Scope

- The `openmixer` application and every crate in this repository.
- The installers built by `.github/workflows/package.yml`, and their signatures and checksums.
- Files the application opens: audio files, library databases, controller maps, themes and plugins.

Plugins run inside the application with its permissions (see `docs/guidelines/extensibility.md`). A plugin doing what it was written to do is not a vulnerability in OpenMixer; a way for a file or a controller map to run code is.

## Supported versions

OpenMixer is in alpha. Fixes go into `main` and the next release; older releases are not patched.
