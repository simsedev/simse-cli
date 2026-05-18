// OpenAI ACP Provider Plugin
// Streams OpenAI Chat Completions via the official `openai` SDK.

// Types: @simse/plugin-sdk (stripped at build time, see tsconfig paths).
// `openai` is loaded at runtime via dynamic import (see openai-sdk.d.ts).

declare const Simse: SimseHost;
declare const Deno: { env: { get(key: string): string | undefined; set(key: string, value: string): void } };

interface ProviderConfig {
	apiKey?: string;
	baseUrl?: string;
	defaultModel?: string;
}

let apiKey = "";
let baseUrl = "https://api.openai.com/v1";
let defaultModel = "gpt-5";

/** Convert core ToolDef[] to OpenAI's function-tool format. */
function toOpenAiTools(
	tools: ToolDef[],
): Array<{ type: "function"; function: { name: string; description: string; parameters: unknown } }> {
	return tools.map((t) => ({
		type: "function" as const,
		function: {
			name: t.name,
			description: t.description,
			parameters: t.parameters,
		},
	}));
}

(globalThis as any).__simsePlugin = ({
	auth: {
		type: "api_key",
		name: "OPENAI_API_KEY",
		description: "OpenAI API key",
		envVar: "OPENAI_API_KEY",
		required: true,
	},

	async initialize(config: ProviderConfig) {
		apiKey = config.apiKey ?? (config as any).__auth?.token ?? Deno.env.get("OPENAI_API_KEY") ?? "";
		baseUrl = config.baseUrl ?? baseUrl;
		defaultModel = config.defaultModel ?? defaultModel;

		if (!apiKey) {
			throw new Error(
				"OPENAI_API_KEY not set. Set it in plugin config or environment.",
			);
		}

		Simse.log("info", `OpenAI plugin initialized (model: ${defaultModel})`);

		return {
			name: "openai",
			version: "0.1.0",
			models: ["gpt-5", "gpt-4.1", "gpt-4.1-mini"],
		};
	},

	async newSession(_sessionId: string, _options: Record<string, unknown>) {
		// OpenAI Chat Completions is stateless — no session management needed.
	},

	async prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	) {
		const OpenAI = (await import("openai")).default;
		const client = new OpenAI({ apiKey, baseURL: baseUrl });

		const model = options.model ?? defaultModel;

		const apiMessages: Array<{ role: string; content: string }> = [];
		if (options.systemPrompt) {
			apiMessages.push({ role: "system", content: options.systemPrompt });
		}
		for (const m of messages) {
			apiMessages.push({ role: m.role, content: m.content });
		}

		const req: Record<string, unknown> = {
			model,
			messages: apiMessages,
			stream: true,
			// include_usage surfaces token counts in a final stream chunk.
			stream_options: { include_usage: true },
		};
		if (options.maxTokens !== undefined) {
			req.max_completion_tokens = options.maxTokens;
		}
		if (options.temperature !== undefined) req.temperature = options.temperature;
		if (options.topP !== undefined) req.top_p = options.topP;

		const tools = options.tools ?? [];
		if (tools.length > 0) {
			req.tools = toOpenAiTools(tools);
		}

		const stream = await client.chat.completions.create(req);

		let stopReason = "end_turn";
		let promptTokens = 0;
		let completionTokens = 0;
		// OpenAI streams tool-call arguments incrementally, keyed by index.
		const toolCalls = new Map<number, { id: string; name: string; args: string }>();

		for await (const chunk of stream) {
			const choice = chunk.choices?.[0];
			if (choice?.delta?.content) {
				Simse.sendDelta(sessionId, choice.delta.content);
			}
			for (const tc of choice?.delta?.tool_calls ?? []) {
				const entry = toolCalls.get(tc.index) ?? { id: "", name: "", args: "" };
				if (tc.id) entry.id = tc.id;
				if (tc.function?.name) entry.name = tc.function.name;
				if (tc.function?.arguments) entry.args += tc.function.arguments;
				toolCalls.set(tc.index, entry);
			}
			if (choice?.finish_reason === "tool_calls") {
				stopReason = "tool_use";
			} else if (choice?.finish_reason === "length") {
				stopReason = "max_tokens";
			}
			if (chunk.usage) {
				promptTokens = chunk.usage.prompt_tokens ?? 0;
				completionTokens = chunk.usage.completion_tokens ?? 0;
			}
		}

		// Emit a <tool_use> block per accumulated tool call for the core parser.
		for (const tc of toolCalls.values()) {
			let args: unknown = {};
			try {
				args = tc.args ? JSON.parse(tc.args) : {};
			} catch {
				args = {};
			}
			const payload = JSON.stringify({
				id: tc.id,
				name: tc.name,
				arguments: args,
			});
			Simse.sendDelta(sessionId, `<tool_use>\n${payload}\n</tool_use>`);
		}

		Simse.sendComplete(sessionId, { promptTokens, completionTokens });

		return { stopReason, usage: { promptTokens, completionTokens } };
	},

	async dispose() {
		Simse.log("info", "OpenAI plugin disposed");
	},
} satisfies AcpPlugin);
