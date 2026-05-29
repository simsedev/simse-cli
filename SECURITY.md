# Security Policy

## Reporting a Vulnerability

Report suspected vulnerabilities in the simse CLI, its plugins, or the
install scripts to **security@simse.dev**. Do not open a public issue for
security reports.

Include: affected component (CLI binary, a named plugin, or an install
script), reproduction steps, and the version or git ref. Expect an
acknowledgement within 3 business days.

## Scope

This repository owns the `simse` CLI crate (the `simse` binary, built here
from `simse-cli` with `simse-core` as a path dependency) plus its packaging:
the plugin SDK, first-party plugins, install scripts, and the release
pipeline. Vulnerabilities in the shared runtime substrate live in the `core`
repository (`simse-core`) — report those against that component.

## Supported Versions

The latest released `v*` tag receives security fixes. Older versions are
not maintained — upgrade via the install script.

## Install Script Integrity

`install.sh` and `install.ps1` download release archives over HTTPS from
GitHub Releases. Inspect a script before piping it to a shell:

    curl -fsSL https://cdn.simse.dev/install.sh | less

Each release publishes a `SHA256SUMS` file alongside the archives. The
install scripts download it and verify the archive's SHA-256 before
extracting; a mismatch aborts the install. Releases predating `SHA256SUMS`
fall back to a warning.
