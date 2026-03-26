// Claude ACP Provider Plugin
// Translates ACP prompts to Anthropic Messages API calls with streaming.

// Types: @simse/plugin-sdk (stripped at build time, see tsconfig paths)


declare const Simse: SimseHost;
declare const Deno: { env: { get(key: string): string | undefined; set(key: string, value: string): void } };

interface ProviderConfig {
	apiKey?: string;
	baseUrl?: string;
	defaultModel?: string;
}

let apiKey = "";
let baseUrl = "https://api.anthropic.com";
let defaultModel = "claude-sonnet-4-20250514";

function parseSSELines(text: string): Array<{ event: string; data: string }> {
	const events: Array<{ event: string; data: string }> = [];
	let currentEvent = "";
	let currentData = "";

	for (const line of text.split("\n")) {
		if (line.startsWith("event: ")) {
			currentEvent = line.slice(7);
		} else if (line.startsWith("data: ")) {
			currentData = line.slice(6);
		} else if (line === "" && currentEvent) {
			events.push({ event: currentEvent, data: currentData });
			currentEvent = "";
			currentData = "";
		}
	}

	return events;
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
			version: "1.0.0",
			models: [
				"claude-sonnet-4-20250514",
				"claude-opus-4-20250514",
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
		const model = options.model ?? defaultModel;
		const maxTokens = options.maxTokens ?? 8192;

		const body: Record<string, unknown> = {
			model,
			max_tokens: maxTokens,
			messages: messages
				.filter((m) => m.role !== "system")
				.map((m) => ({ role: m.role, content: m.content })),
			stream: true,
		};

		const systemMsg = messages.find((m) => m.role === "system");
		if (options.systemPrompt) {
			body.system = options.systemPrompt;
		} else if (systemMsg) {
			body.system = systemMsg.content;
		}

		if (options.temperature !== undefined)
			body.temperature = options.temperature;
		if (options.topP !== undefined) body.top_p = options.topP;

		const response = await fetch(`${baseUrl}/v1/messages`, {
			method: "POST",
			headers: {
				"x-api-key": apiKey,
				"anthropic-version": "2023-06-01",
				"content-type": "application/json",
			},
			body: JSON.stringify(body),
		});

		if (!response.ok) {
			const errorText = await response.text();
			throw new Error(
				`Anthropic API error ${response.status}: ${errorText}`,
			);
		}

		const reader = response.body!.getReader();
		const decoder = new TextDecoder();
		let buffer = "";
		let promptTokens = 0;
		let completionTokens = 0;
		let stopReason = "end_turn";

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;

			buffer += decoder.decode(value, { stream: true });

			const lastNewline = buffer.lastIndexOf("\n\n");
			if (lastNewline === -1) continue;

			const complete = buffer.slice(0, lastNewline + 2);
			buffer = buffer.slice(lastNewline + 2);

			for (const event of parseSSELines(complete)) {
				if (event.event === "content_block_delta") {
					const data = JSON.parse(event.data);
					if (data.delta?.type === "text_delta" && data.delta.text) {
						Simse.sendDelta(sessionId, data.delta.text);
					}
				} else if (event.event === "message_delta") {
					const data = JSON.parse(event.data);
					if (data.delta?.stop_reason)
						stopReason = data.delta.stop_reason;
					if (data.usage)
						completionTokens = data.usage.output_tokens ?? 0;
				} else if (event.event === "message_start") {
					const data = JSON.parse(event.data);
					if (data.message?.usage)
						promptTokens = data.message.usage.input_tokens ?? 0;
				}
			}
		}

		Simse.sendComplete(sessionId, { promptTokens, completionTokens });

		return { stopReason, usage: { promptTokens, completionTokens } };
	},

	async dispose() {
		Simse.log("info", "Claude plugin disposed");
	},
} satisfies AcpPlugin);
