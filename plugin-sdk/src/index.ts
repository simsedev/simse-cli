// Simse Plugin SDK
// Shared types and registration helper for all simse plugins.

// --- Auth ---

export interface PluginAuth {
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

// --- Plugin info ---

export interface PluginInfo {
	name: string;
	version: string;
	models?: string[];
	tools?: McpToolDef[];
	resources?: McpResourceDef[];
}

// --- Messages ---

export interface PluginMessage {
	role: string;
	content: string;
}

// --- Tools ---

export interface ToolDef {
	name: string;
	description: string;
	parameters: Record<string, unknown>;
}

export interface PromptOptions {
	model?: string;
	systemPrompt?: string;
	temperature?: number;
	topP?: number;
	maxTokens?: number;
	tools?: ToolDef[];
}

export interface PromptResult {
	stopReason: string;
	usage?: TokenUsage | null;
}

export interface TokenUsage {
	promptTokens: number;
	completionTokens: number;
}

// --- MCP ---

export interface McpToolDef {
	name: string;
	description: string;
	inputSchema: Record<string, unknown>;
}

export interface McpToolResult {
	content: Array<
		| { type: 'text'; text: string }
		| { type: 'image'; data: string; mimeType: string }
	>;
	isError?: boolean;
}

export interface McpResourceDef {
	uri: string;
	name: string;
	description?: string;
	mimeType?: string;
}

export interface McpResourceResult {
	contents: Array<{ uri: string; text?: string; mimeType?: string }>;
}

// --- Host APIs (injected by runtime) ---

export interface SimseHost {
	sendDelta(sessionId: string, text: string): void;
	sendComplete(sessionId: string, usage?: TokenUsage | null): void;
	log(level: 'debug' | 'info' | 'warn' | 'error', message: string): void;
}

// --- Base plugin interface ---

export interface SimsePlugin {
	kind?: 'acp' | 'mcp';
	auth: PluginAuth | PluginAuth[];
	initialize(config: Record<string, unknown>): Promise<PluginInfo>;
	dispose?(): Promise<void>;
}

// --- ACP provider plugin ---

export interface AcpPlugin extends SimsePlugin {
	newSession?(sessionId: string, options: Record<string, unknown>): Promise<void>;
	prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	): Promise<PromptResult>;
}

// --- MCP tool plugin ---

export interface McpPlugin extends SimsePlugin {
	tools(): McpToolDef[];
	callTool(name: string, args: Record<string, unknown>): Promise<McpToolResult>;
	resources?(): McpResourceDef[];
	readResource?(uri: string): Promise<McpResourceResult>;
}

// --- Registration ---

/** Register a plugin with the simse runtime. */
export function registerPlugin<T extends SimsePlugin>(plugin: T): void {
	(globalThis as Record<string, unknown>).__simsePlugin = plugin;
}
