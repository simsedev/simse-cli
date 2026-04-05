# CLAUDE.md

Guide for agents working in `simse-cli/`.

## Overview

`simse-cli` is the CLI packaging repo for simse. It contains the plugin SDK, first-party plugins, and cross-platform install scripts. The Rust CLI binary itself is built from `core/`; this repo owns the plugin ecosystem and distribution.

## Structure

```text
simse-cli/
  plugin-sdk/         shared TypeScript SDK for plugin authors
  plugins/            first-party plugins (ACP, MCP, skills, hooks)
  scripts/            cross-platform install scripts (install.sh, install.ps1)
```

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
| `claude` | acp | Anthropic Claude provider via Messages API |
| `copilot` | acp | GitHub Copilot provider via SDK |
| `ollama` | acp | Local Ollama server provider |
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
