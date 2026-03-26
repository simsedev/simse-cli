// Simse Plugin SDK type definitions.
// These types are injected by the Deno runtime at load time.

/** Host APIs provided by the simse plugin runtime. */
declare namespace Simse {
	/** Send a streaming text delta to the session. */
	function sendDelta(sessionId: string, text: string): void;
	/** Signal completion of a streaming response. */
	function sendComplete(
		sessionId: string,
		usage?: { promptTokens: number; completionTokens: number } | null,
	): void;
	/** Log a message at the given level. */
	function log(level: 'debug' | 'info' | 'warn' | 'error', message: string): void;
}

/** Deno runtime APIs available in the plugin sandbox. */
declare namespace Deno {
	/** Environment variable access. */
	const env: {
		get(key: string): string | undefined;
		set(key: string, value: string): void;
	};
}

/** Extend globalThis to include the plugin registration property. */
declare var __simsePlugin: SimsePlugin;

/** Base plugin interface — all plugins must implement this. */
interface SimsePlugin {
	kind?: 'acp' | 'mcp';
	auth: PluginAuth | PluginAuth[];
	initialize(config: Record<string, unknown>): Promise<PluginInfo>;
	dispose?(): Promise<void>;
	newSession?(sessionId: string, options: Record<string, unknown>): Promise<void>;
	prompt?(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	): Promise<PromptResult>;
	callTool?(name: string, args: Record<string, unknown>): Promise<McpToolResult>;
	readResource?(uri: string): Promise<McpResourceResult>;
}

/** ACP provider plugin — adds session and prompt methods. */
interface AcpPlugin extends SimsePlugin {
	prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	): Promise<PromptResult>;
}

/** MCP tool plugin — adds tool listing and execution. */
interface McpPlugin extends SimsePlugin {
	callTool(name: string, args: Record<string, unknown>): Promise<McpToolResult>;
}

interface PluginInfo {
	name: string;
	version: string;
	models?: string[];
	tools?: McpToolDef[];
	resources?: McpResourceDef[];
}

interface PluginAuth {
	type: 'none' | 'api_key' | 'oauth' | 'sdk_managed';
	name?: string;
	description?: string;
	envVar?: string;
	required?: boolean;
	provider?: string;
	clientId?: string;
	scopes?: string[];
	deviceAuthUrl?: string;
	tokenUrl?: string;
}

interface PluginMessage {
	role: string;
	content: string;
}

interface ToolDef {
	name: string;
	description: string;
	parameters: Record<string, unknown>;
}

interface PromptOptions {
	model?: string;
	systemPrompt?: string;
	temperature?: number;
	topP?: number;
	maxTokens?: number;
	tools?: ToolDef[];
}

interface PromptResult {
	stopReason: string;
	usage?: { promptTokens: number; completionTokens: number } | null;
}

interface McpToolDef {
	name: string;
	description: string;
	inputSchema: Record<string, unknown>;
}

interface McpToolResult {
	content: Array<{ type: 'text'; text: string } | { type: 'image'; data: string; mimeType: string }>;
	isError?: boolean;
}

interface McpResourceDef {
	uri: string;
	name: string;
	description?: string;
	mimeType?: string;
}

interface McpResourceResult {
	contents: Array<{ uri: string; text?: string; mimeType?: string }>;
}

/** Stub for @github/copilot-sdk (loaded at runtime by the Copilot plugin). */
declare module '@github/copilot-sdk' {
	export class CopilotClient {
		constructor(opts?: Record<string, unknown>);
		start(): Promise<void>;
		stop(): Promise<void>;
		createSession(opts: Record<string, unknown>): Promise<any>;
	}
}
