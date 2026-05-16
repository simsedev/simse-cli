# Changelog

All notable changes to the simse CLI distribution are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions track the `v*` git tags that trigger a release build.

## [Unreleased]

### Added
- GitHub Actions release workflow: 5-target build of `simse` from `core`,
  plugin bundling, and GitHub Release publishing on `v*` tags.
- bun workspace and `tsc` typecheck coverage for the plugin SDK and the
  five TypeScript plugins.
- `plugin.json` manifest validation (`scripts/validate-plugins.sh`).

### Changed
- Install scripts now deploy bundled plugins to `<prefix>/share/simse/plugins`
  alongside the binary.

## [0.2.0] - prior

- Plugin SDK, first-party plugins, and cross-platform install scripts.
