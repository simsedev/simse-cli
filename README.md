# simse-cli

CLI packaging, plugin SDK, and first-party plugins for simse.

## Install

```bash
# macOS / Linux
curl -fsSL https://cdn.simse.dev/install.sh | sh

# Windows (PowerShell)
irm https://cdn.simse.dev/install.ps1 | iex
```

## Plugins

| Plugin | Kind | Description |
|--------|------|-------------|
| `claude` | ACP | Anthropic Claude provider via Messages API |
| `copilot` | ACP | GitHub Copilot provider via SDK |
| `ollama` | ACP | Local Ollama server provider |
| `github` | MCP | GitHub REST API tools and resources |
| `perplexity` | MCP | Perplexity Sonar web search tool |
| `code-review` | Skill | Structured code review with checklist |
| `format-on-save` | Hook | Auto-format files after tool writes |

## Plugin SDK

The `@simse/plugin-sdk` package in `plugin-sdk/` provides TypeScript interfaces for building plugins:

- **ACP plugins** implement `AcpPlugin` for LLM providers (prompt, session management)
- **MCP plugins** implement `McpPlugin` for tools and resources
- **Skills** define structured prompts via `SKILL.md`
- **Hooks** define event triggers via `hooks.toml`

## Structure

```text
plugin-sdk/       @simse/plugin-sdk — shared TypeScript interfaces
plugins/
  claude/         Anthropic Claude provider
  copilot/        GitHub Copilot provider
  ollama/         Local Ollama provider
  github/         GitHub REST API tools
  perplexity/     Perplexity web search
  code-review/    Code review skill
  format-on-save/ Auto-format hook
scripts/
  install.sh      Unix installer (macOS, Linux)
  install.ps1     Windows PowerShell installer
```
