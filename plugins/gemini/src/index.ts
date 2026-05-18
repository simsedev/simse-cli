// Gemini ACP Provider Plugin
// Streams Google Gemini responses via the official @google/genai SDK.

// Types: @simse/plugin-sdk (stripped at build time, see tsconfig paths).
// @google/genai is loaded at runtime via dynamic import (see gemini-sdk.d.ts).

declare const Simse: SimseHost;
declare const Deno: { env: { get(key: string): string | undefined; set(key: string, value: string): void } };

interface ProviderConfig {
	apiKey?: string;
	defaultModel?: string;
}

let apiKey = "";
let defaultModel = "gemini-2.5-flash";

/** Convert core ToolDef[] to Gemini's functionDeclarations format. */
function toGeminiTools(
	tools: ToolDef[],
): Array<{ name: string; description: string; parameters: unknown }> {
	return tools.map((t) => ({
		name: t.name,
		description: t.description,
		parameters: t.parameters,
	}));
}

(globalThis as any).__simsePlugin = ({
	auth: {
		type: "api_key",
		name: "GEMINI_API_KEY",
		description: "Google Gemini API key",
		envVar: "GEMINI_API_KEY",
		required: true,
	},

	async initialize(config: ProviderConfig) {
		apiKey = config.apiKey ?? (config as any).__auth?.token ?? Deno.env.get("GEMINI_API_KEY") ?? "";
		defaultModel = config.defaultModel ?? defaultModel;

		if (!apiKey) {
			throw new Error(
				"GEMINI_API_KEY not set. Set it in plugin config or environment.",
			);
		}

		Simse.log("info", `Gemini plugin initialized (model: ${defaultModel})`);

		return {
			name: "gemini",
			version: "0.1.0",
			models: ["gemini-2.5-pro", "gemini-2.5-flash"],
		};
	},

	async newSession(_sessionId: string, _options: Record<string, unknown>) {
		// Gemini's generateContent API is stateless — no session management needed.
	},

	async prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	) {
		const { GoogleGenAI } = await import("@google/genai");
		const ai = new GoogleGenAI({ apiKey });

		const model = options.model ?? defaultModel;

		// Gemini turns are `user` / `model`; system turns go in
		// systemInstruction, tool turns fold into `user`.
		const contents = messages
			.filter((m) => m.role !== "system")
			.map((m) => ({
				role: m.role === "assistant" ? "model" : "user",
				parts: [{ text: m.content }],
			}));

		const config: Record<string, unknown> = {};
		const systemText =
			options.systemPrompt ??
			messages.find((m) => m.role === "system")?.content;
		if (systemText) config.systemInstruction = systemText;
		if (options.temperature !== undefined) config.temperature = options.temperature;
		if (options.topP !== undefined) config.topP = options.topP;
		if (options.maxTokens !== undefined) config.maxOutputTokens = options.maxTokens;

		const tools = options.tools ?? [];
		if (tools.length > 0) {
			config.tools = [{ functionDeclarations: toGeminiTools(tools) }];
		}

		const stream = await ai.models.generateContentStream({
			model,
			contents,
			config,
		});

		let promptTokens = 0;
		let completionTokens = 0;
		const toolCalls: Array<{ name: string; args: Record<string, unknown> }> = [];

		for await (const chunk of stream) {
			if (chunk.text) {
				Simse.sendDelta(sessionId, chunk.text);
			}
			for (const fc of chunk.functionCalls ?? []) {
				toolCalls.push({ name: fc.name ?? "", args: fc.args ?? {} });
			}
			if (chunk.usageMetadata) {
				promptTokens = chunk.usageMetadata.promptTokenCount ?? promptTokens;
				completionTokens =
					chunk.usageMetadata.candidatesTokenCount ?? completionTokens;
			}
		}

		// Emit a <tool_use> block per function call for the core parser.
		let callIdx = 1;
		for (const tc of toolCalls) {
			const payload = JSON.stringify({
				id: `call_${callIdx++}`,
				name: tc.name,
				arguments: tc.args,
			});
			Simse.sendDelta(sessionId, `<tool_use>\n${payload}\n</tool_use>`);
		}

		const stopReason = toolCalls.length > 0 ? "tool_use" : "end_turn";
		Simse.sendComplete(sessionId, { promptTokens, completionTokens });

		return { stopReason, usage: { promptTokens, completionTokens } };
	},

	async dispose() {
		Simse.log("info", "Gemini plugin disposed");
	},
} satisfies AcpPlugin);
