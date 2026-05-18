# Changelog

All notable changes to the simse CLI distribution are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions track the `v*` git tags that trigger a release build.

## [Unreleased]

### Added
- GitHub Actions release workflow: 6-target build of `simse` from `core`,
  plugin bundling, and GitHub Release publishing on `v*` tags. Added the
  `windows-aarch64` target so the Windows ARM path in `install.ps1` resolves.
- bun workspace and `tsc` typecheck coverage for the plugin SDK and the
  five TypeScript plugins.
- `plugin.json` manifest validation (`scripts/validate-plugins.sh`).
- Release workflow now runs plugin validation and typecheck (`check` job)
  before any platform build.
- Claude provider plugin now supports tool calling: `options.tools` is sent
  to the Messages API and streamed `tool_use` blocks are emitted as
  `<tool_use>` blocks for the core tool-call parser.
- Releases publish a `SHA256SUMS` file; `install.sh` and `install.ps1`
  verify the downloaded archive against it before extracting.

### Changed
- Install scripts now deploy bundled plugins to `<prefix>/share/simse/plugins`
  alongside the binary.
- `CORE_REF` is pinned to a `core` commit SHA (was the moving `main` branch)
  so a given release tag builds reproducibly.
- `assemble-dist.sh` bundles the copilot plugin's runtime dependency
  (`@github/copilot-sdk`) into the dist plugin directory.
- github plugin: dropped the placeholder OAuth auth entry; `GITHUB_TOKEN`
  API-key auth is the single auth method.
- claude plugin: model list refreshed to current models
  (`claude-opus-4-7`, `claude-sonnet-4-6`, `claude-haiku-4-5-20251001`);
  default model is now `claude-sonnet-4-6`.

### Fixed
- `install.sh` no longer removes the existing `simse` binary before the new
  download succeeds — a failed download previously left no binary installed.
- github plugin `tools()` now returns the tool list instead of an empty
  array, matching the perplexity plugin and the `McpPlugin` contract.
- copilot plugin declares its `@github/copilot-sdk` dependency.
- Plugin `initialize()` results report version `0.1.0`, matching their
  `plugin.json` manifests (previously hardcoded `1.0.0`).
- perplexity plugin sends the valid `search_mode` parameter for academic
  search instead of the non-existent `search_focus` field.
- `format-on-save` cleans up its temp file when JSON formatting fails.

## [0.2.0] - prior

- Plugin SDK, first-party plugins, and cross-platform install scripts.
