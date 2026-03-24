// Ollama ACP Provider Plugin
// Calls Ollama's native /api/chat endpoint with streaming.

declare namespace Simse {
	function sendDelta(sessionId: string, text: string): void;
	function sendComplete(
		sessionId: string,
		usage?: { promptTokens: number; completionTokens: number } | null,
	): void;
	function log(level: string, message: string): void;
}

interface Message {
	role: string;
	content: string;
}

interface PromptOptions {
	model?: string;
	systemPrompt?: string;
	temperature?: number;
	topP?: number;
	maxTokens?: number;
}

interface ProviderConfig {
	baseUrl?: string;
	defaultModel?: string;
}

let baseUrl = "http://localhost:11434";
let defaultModel = "llama3.1";

globalThis.__simsePlugin = {
	auth: { type: "none" },

	async initialize(config: ProviderConfig) {
		baseUrl = config.baseUrl ?? baseUrl;
		defaultModel = config.defaultModel ?? defaultModel;

		try {
			const healthResp = await fetch(`${baseUrl}/api/tags`);
			if (!healthResp.ok) throw new Error(`HTTP ${healthResp.status}`);
			const data = await healthResp.json();
			const models = (data.models ?? []).map((m: any) => m.name as string);

			Simse.log("info", `Ollama plugin initialized (${models.length} models available)`);

			return {
				name: "ollama",
				version: "1.0.0",
				models: models.length > 0 ? models : [defaultModel],
			};
		} catch (e) {
			Simse.log("warn", `Ollama not reachable at ${baseUrl}: ${e}. Will retry on first prompt.`);
			return {
				name: "ollama",
				version: "1.0.0",
				models: [defaultModel],
			};
		}
	},

	async newSession(_sessionId: string, _options: Record<string, unknown>) {
		// Ollama API is stateless.
	},

	async prompt(sessionId: string, messages: Message[], options: PromptOptions) {
		const model = options.model ?? defaultModel;

		const apiMessages: Array<{ role: string; content: string }> = [];
		if (options.systemPrompt) {
			apiMessages.push({ role: "system", content: options.systemPrompt });
		}
		for (const m of messages) {
			apiMessages.push({ role: m.role, content: m.content });
		}

		const body: Record<string, unknown> = {
			model,
			messages: apiMessages,
			stream: true,
		};

		if (options.temperature !== undefined) {
			body.options = { ...(body.options as Record<string, unknown> ?? {}), temperature: options.temperature };
		}
		if (options.topP !== undefined) {
			body.options = { ...(body.options as Record<string, unknown> ?? {}), top_p: options.topP };
		}
		if (options.maxTokens !== undefined) {
			body.options = { ...(body.options as Record<string, unknown> ?? {}), num_predict: options.maxTokens };
		}

		const response = await fetch(`${baseUrl}/api/chat`, {
			method: "POST",
			headers: { "content-type": "application/json" },
			body: JSON.stringify(body),
		});

		if (!response.ok) {
			const errorText = await response.text();
			throw new Error(`Ollama API error ${response.status}: ${errorText}`);
		}

		// Ollama streams newline-delimited JSON objects.
		// Each object has: { model, message: { role, content }, done, ...metrics }
		// When done=true, the final object includes eval_count, prompt_eval_count, etc.
		const reader = response.body!.getReader();
		const decoder = new TextDecoder();
		let buffer = "";
		let stopReason = "end_turn";
		let promptTokens = 0;
		let completionTokens = 0;

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;

			buffer += decoder.decode(value, { stream: true });

			const lines = buffer.split("\n");
			buffer = lines.pop() ?? "";

			for (const line of lines) {
				const trimmed = line.trim();
				if (!trimmed) continue;

				try {
					const data = JSON.parse(trimmed);

					// Stream content delta
					if (data.message?.content) {
						Simse.sendDelta(sessionId, data.message.content);
					}

					// Final message with metrics
					if (data.done === true) {
						if (data.done_reason === "length") {
							stopReason = "max_tokens";
						}
						promptTokens = data.prompt_eval_count ?? 0;
						completionTokens = data.eval_count ?? 0;
					}
				} catch {
					// Skip unparseable lines.
				}
			}
		}

		const usage =
			promptTokens > 0 || completionTokens > 0
				? { promptTokens, completionTokens }
				: null;

		Simse.sendComplete(sessionId, usage);

		return { stopReason, usage };
	},

	async dispose() {
		Simse.log("info", "Ollama plugin disposed");
	},
};
