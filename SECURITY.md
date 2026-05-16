# Security Policy

## Reporting a Vulnerability

Report suspected vulnerabilities in the simse CLI, its plugins, or the
install scripts to **security@simse.dev**. Do not open a public issue for
security reports.

Include: affected component (CLI binary, a named plugin, or an install
script), reproduction steps, and the version or git ref. Expect an
acknowledgement within 3 business days.

## Scope

This repository covers CLI packaging: the plugin SDK, first-party plugins,
install scripts, and the release pipeline. The `simse` binary itself is
built from the `core` repository — report core runtime vulnerabilities
against that component.

## Supported Versions

The latest released `v*` tag receives security fixes. Older versions are
not maintained — upgrade via the install script.

## Install Script Integrity

`install.sh` and `install.ps1` download release archives over HTTPS from
GitHub Releases. Inspect a script before piping it to a shell:

    curl -fsSL https://cdn.simse.dev/install.sh | less
