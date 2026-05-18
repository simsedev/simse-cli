// Claude ACP Provider Plugin
// Streams Anthropic Messages API responses via the official @anthropic-ai/sdk.

// Types: @simse/plugin-sdk (stripped at build time, see tsconfig paths).
// @anthropic-ai/sdk is loaded at runtime via dynamic import (see the .d.ts).

declare const Simse: SimseHost;
declare const Deno: { env: { get(key: string): string | undefined; set(key: string, value: string): void } };

interface ProviderConfig {
	apiKey?: string;
	baseUrl?: string;
	defaultModel?: string;
}

let apiKey = "";
let baseUrl = "https://api.anthropic.com";
let defaultModel = "claude-sonnet-4-6";

/** Convert core ToolDef[] to Anthropic's Messages API tool format. */
function toAnthropicTools(
	tools: ToolDef[],
): Array<{ name: string; description: string; input_schema: Record<string, unknown> }> {
	return tools.map((t) => ({
		name: t.name,
		description: t.description,
		input_schema: t.parameters,
	}));
}

(globalThis as any).__simsePlugin = ({
	auth: {
		type: "api_key",
		name: "ANTHROPIC_API_KEY",
		description: "Anthropic API key for Claude",
		envVar: "ANTHROPIC_API_KEY",
		required: true,
	},

	async initialize(config: ProviderConfig) {
		apiKey = config.apiKey ?? (config as any).__auth?.token ?? Deno.env.get("ANTHROPIC_API_KEY") ?? "";
		baseUrl = config.baseUrl ?? baseUrl;
		defaultModel = config.defaultModel ?? defaultModel;

		if (!apiKey) {
			throw new Error(
				"ANTHROPIC_API_KEY not set. Set it in plugin config or environment.",
			);
		}

		Simse.log("info", `Claude plugin initialized (model: ${defaultModel})`);

		return {
			name: "claude",
			version: "0.1.0",
			models: [
				"claude-opus-4-7",
				"claude-sonnet-4-6",
				"claude-haiku-4-5-20251001",
			],
		};
	},

	async newSession(_sessionId: string, _options: Record<string, unknown>) {
		// Claude Messages API is stateless — no session management needed.
	},

	async prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	) {
		const Anthropic = (await import("@anthropic-ai/sdk")).default;
		const client = new Anthropic({ apiKey, baseURL: baseUrl });

		const model = options.model ?? defaultModel;
		const maxTokens = options.maxTokens ?? 8192;

		const params: Record<string, unknown> = {
			model,
			max_tokens: maxTokens,
			messages: messages
				.filter((m) => m.role !== "system")
				.map((m) => ({ role: m.role, content: m.content })),
		};

		// System prompt as a cache-controlled text block: repeated turns in an
		// agentic loop reuse the cached prefix instead of re-billing it.
		const systemText =
			options.systemPrompt ??
			messages.find((m) => m.role === "system")?.content;
		if (systemText) {
			params.system = [
				{
					type: "text",
					text: systemText,
					cache_control: { type: "ephemeral" },
				},
			];
		}

		// temperature / top_p are rejected by Opus 4.7 — only forward them for
		// models that still accept sampling parameters.
		if (!model.includes("opus-4-7")) {
			if (options.temperature !== undefined) {
				params.temperature = options.temperature;
			}
			if (options.topP !== undefined) {
				params.top_p = options.topP;
			}
		}

		// Pass tools in Anthropic's native format if provided.
		const tools = options.tools ?? [];
		if (tools.length > 0) {
			params.tools = toAnthropicTools(tools);
		}

		// Stream text deltas to the UI as they arrive; the SDK accumulates the
		// full message for us.
		const stream = client.messages.stream(params);
		stream.on("text", (delta: string) => {
			Simse.sendDelta(sessionId, delta);
		});

		const final = await stream.finalMessage();

		// Emit any tool_use blocks as <tool_use> blocks for the core parser.
		for (const block of final.content ?? []) {
			if (block.type === "tool_use") {
				const payload = JSON.stringify({
					id: block.id,
					name: block.name,
					arguments: (block as any).input ?? {},
				});
				Simse.sendDelta(sessionId, `<tool_use>\n${payload}\n</tool_use>`);
			}
		}

		const promptTokens = final.usage?.input_tokens ?? 0;
		const completionTokens = final.usage?.output_tokens ?? 0;
		const stopReason = final.stop_reason ?? "end_turn";

		Simse.sendComplete(sessionId, { promptTokens, completionTokens });

		return { stopReason, usage: { promptTokens, completionTokens } };
	},

	async dispose() {
		Simse.log("info", "Claude plugin disposed");
	},
} satisfies AcpPlugin);
