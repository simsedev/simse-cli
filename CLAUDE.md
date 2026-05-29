# CLAUDE.md

Guide for agents working in `simse-cli/`.

## Overview

`simse-cli` is the home of the `simse` CLI / agent binary plus its packaging: the plugin SDK, first-party plugins, and cross-platform install scripts. The Rust CLI crate was extracted from `core/src/cli` in the final phase of the core service-split (per `docs/superpowers/specs/2026-05-29-core-service-split-design.md`), leaving `core` a pure library with zero binaries. The CLI consumes the shared substrate (tools, remote, agentic_loop, plugin_manager, error, conversation, config, acp, memory_sync, tasks, prompts, plugin_tools, permissions) from `simse-core` via a path dependency; this repo also owns the plugin ecosystem and distribution.

## Structure

```text
simse-cli/
  Cargo.toml          the `simse-cli` crate ([[bin]] simse)
  src/                CLI source (lib.rs crate root + main.rs entry)
  plugin-sdk/         shared TypeScript SDK for plugin authors
  plugins/            first-party plugins (ACP, MCP, skills, hooks)
  scripts/            cross-platform install scripts (install.sh, install.ps1)
```

## Build

```
cargo build --release   # produces target/release/simse
```

The `simse-core` path dep is enabled with `default-features = false` and features `remote, plugins, sandbox, scheduler, adaptive`. The crate declares local `plugins` + `adaptive` feature markers (default-on) that gate the copied `#[cfg(feature = "...")]` code. The CLI owns the cloud-backed memory tools (`src/tools/memory.rs`), the sub-agent / ACP-delegate runners (`src/tools/subagent_cli.rs`), and the gRPC-Web `AdaptiveService` client (`src/memory_client.rs`) — these were re-homed from `core/src` in the core purify cut (core commit 2cc29b3), so the CLI no longer requests core's removed `cli` feature. `build.rs` compiles the one proto the CLI owns locally, `quantiz/adaptive.proto` (from `../foundry/proto`), into `src/proto/` for that memory client; the inference proto types still come re-exported from `simse-core` under its `remote` feature.

## Plugin Types

| Kind | Interface | Purpose |
| --- | --- | --- |
| `acp` | `AcpPlugin` | LLM provider plugins (prompt, session) |
| `mcp` | `McpPlugin` | Tool and resource plugins (tools, callTool, resources) |
| `skill` | SKILL.md | Structured skill definitions |
| `hook` | hooks.toml | Event-triggered automation |

## Plugins

| Plugin | Kind | Description |
| --- | --- | --- |
| `claude` | acp | Anthropic Claude provider via the Anthropic SDK |
| `copilot` | acp | GitHub Copilot provider via SDK |
| `gemini` | acp | Google Gemini provider via the @google/genai SDK |
| `ollama` | acp | Local Ollama server provider |
| `openai` | acp | OpenAI provider via the OpenAI SDK |
| `github` | mcp | GitHub REST API tools and resources |
| `perplexity` | mcp | Perplexity Sonar web search tool |
| `code-review` | skill | Structured code review with checklist |
| `format-on-save` | hook | Auto-format files after tool writes |

## Plugin SDK

`@simse/plugin-sdk` in `plugin-sdk/` defines the TypeScript interfaces (`SimsePlugin`, `AcpPlugin`, `McpPlugin`, `PluginAuth`, etc.) that all plugins implement. Plugins register via `(globalThis as any).__simsePlugin` and receive host APIs through `Simse` and `Deno` globals.

## Notes

- Plugins use workspace dependencies (`workspace:*` for the SDK).
- Each plugin has a `plugin.json` declaring its `kind`, `name`, and `main` entry point.
- Install scripts download the latest release from GitHub and add the binary to PATH.
