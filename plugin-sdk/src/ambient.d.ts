// Ambient global declarations for the script-mode plugin runtime.
//
// The Deno runtime loads each plugin's `src/index.ts` as a classic script,
// so plugin source contains no `import` statements. Plugins reference SDK
// types as ambient global names (e.g. `declare const Simse: SimseHost`).
// This file re-exposes the module-scoped SDK interfaces in the global scope
// for `tsc` to resolve. It carries no runtime code.

import type * as SDK from './index.ts';

declare global {
	type PluginAuth = SDK.PluginAuth;
	type PluginInfo = SDK.PluginInfo;
	type PluginMessage = SDK.PluginMessage;
	type ToolDef = SDK.ToolDef;
	type PromptOptions = SDK.PromptOptions;
	type PromptResult = SDK.PromptResult;
	type TokenUsage = SDK.TokenUsage;
	type McpToolDef = SDK.McpToolDef;
	type McpToolResult = SDK.McpToolResult;
	type McpResourceDef = SDK.McpResourceDef;
	type McpResourceResult = SDK.McpResourceResult;
	type SimseHost = SDK.SimseHost;
	type SimsePlugin = SDK.SimsePlugin;
	type AcpPlugin = SDK.AcpPlugin;
	type McpPlugin = SDK.McpPlugin;
}
