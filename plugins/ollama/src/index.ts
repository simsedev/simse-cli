// Ollama ACP Provider Plugin
// Calls Ollama's native /api/chat endpoint with streaming and tool calling.

import type { AcpPlugin, PluginMessage, PromptOptions, ToolDef, SimseHost } from '@simse/plugin-sdk';


declare const Simse: SimseHost;

/** Convert core ToolDef[] to Ollama's native tool format. */
function toOllamaTools(
	tools: ToolDef[],
): Array<{ type: 'function'; function: { name: string; description: string; parameters: unknown } }> {
	return tools.map((t) => ({
		type: 'function' as const,
		function: {
			name: t.name,
			description: t.description,
			parameters: t.parameters,
		},
	}));
}

/** Format tool calls from Ollama's response as <tool_use> blocks for the core parser. */
function formatToolCallsAsXml(
	toolCalls: Array<{ function: { name: string; arguments: Record<string, unknown> } }>,
): string {
	let callIdx = 1;
	return toolCalls
		.map((tc) => {
			const block = JSON.stringify({
				id: `call_${callIdx++}`,
				name: tc.function.name,
				arguments: tc.function.arguments,
			});
			return `<tool_use>\n${block}\n</tool_use>`;
		})
		.join('\n');
}

let baseUrl = 'http://localhost:11434';
let defaultModel = 'gpt-oss:latest';

(globalThis as any).__simsePlugin = ({
	auth: { type: 'none' },

	async initialize(config: Record<string, unknown>) {
		baseUrl = (config.baseUrl as string) ?? baseUrl;
		defaultModel = (config.defaultModel as string) ?? defaultModel;

		try {
			const healthResp = await fetch(`${baseUrl}/api/tags`);
			if (!healthResp.ok) throw new Error(`HTTP ${healthResp.status}`);
			const data = await healthResp.json();
			const models = (data.models ?? []).map((m: any) => m.name as string);

			Simse.log(
				'info',
				`Ollama plugin initialized (${models.length} models available)`,
			);

			return {
				name: 'ollama',
				version: '1.0.0',
				models: models.length > 0 ? models : [defaultModel],
			};
		} catch (e) {
			Simse.log(
				'warn',
				`Ollama not reachable at ${baseUrl}: ${e}. Will retry on first prompt.`,
			);
			return {
				name: 'ollama',
				version: '1.0.0',
				models: [defaultModel],
			};
		}
	},

	async newSession(_sessionId: string, _options: Record<string, unknown>) {
		// Ollama API is stateless.
	},

	async prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	) {
		const model = options.model ?? defaultModel;

		const apiMessages: Array<{ role: string; content: string }> = [];
		if (options.systemPrompt) {
			apiMessages.push({ role: 'system', content: options.systemPrompt });
		}
		for (const m of messages) {
			apiMessages.push({ role: m.role, content: m.content });
		}

		const body: Record<string, unknown> = {
			model,
			messages: apiMessages,
			stream: true,
		};

		// Pass tools in Ollama's native format if provided.
		const tools = options.tools ?? [];
		if (tools.length > 0) {
			body.tools = toOllamaTools(tools);
		}

		if (options.temperature !== undefined) {
			body.options = {
				...((body.options as Record<string, unknown>) ?? {}),
				temperature: options.temperature,
			};
		}
		if (options.topP !== undefined) {
			body.options = {
				...((body.options as Record<string, unknown>) ?? {}),
				top_p: options.topP,
			};
		}
		if (options.maxTokens !== undefined) {
			body.options = {
				...((body.options as Record<string, unknown>) ?? {}),
				num_predict: options.maxTokens,
			};
		}

		const response = await fetch(`${baseUrl}/api/chat`, {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(body),
		});

		if (!response.ok) {
			const errorText = await response.text();
			throw new Error(`Ollama API error ${response.status}: ${errorText}`);
		}

		// Ollama streams newline-delimited JSON objects.
		// Each object has: { model, message: { role, content, tool_calls? }, done, ...metrics }
		// When done=true, the final object includes eval_count, prompt_eval_count, etc.
		const reader = response.body!.getReader();
		const decoder = new TextDecoder();
		let buffer = '';
		let stopReason = 'end_turn';
		let promptTokens = 0;
		let completionTokens = 0;

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;

			buffer += decoder.decode(value, { stream: true });

			const lines = buffer.split('\n');
			buffer = lines.pop() ?? '';

			for (const line of lines) {
				const trimmed = line.trim();
				if (!trimmed) continue;

				try {
					const data = JSON.parse(trimmed);

					// Stream content delta (text response)
					if (data.message?.content) {
						Simse.sendDelta(sessionId, data.message.content);
					}

					// Native tool calls from the model
					if (
						data.message?.tool_calls &&
						Array.isArray(data.message.tool_calls) &&
						data.message.tool_calls.length > 0
					) {
						const xml = formatToolCallsAsXml(data.message.tool_calls);
						Simse.sendDelta(sessionId, xml);
						stopReason = 'tool_use';
					}

					// Final message with metrics
					if (data.done === true) {
						if (data.done_reason === 'length') {
							stopReason = 'max_tokens';
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
		Simse.log('info', 'Ollama plugin disposed');
	},
} satisfies AcpPlugin);
