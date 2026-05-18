/** Stub for the `openai` package (loaded at runtime by the OpenAI plugin).
 *
 * The Deno runtime resolves the dynamic `import("openai")` against the
 * installed package; this declaration only satisfies `tsc` for the surface
 * the plugin uses — the default `OpenAI` export and the streaming
 * `chat.completions.create()` chunk shape. */
declare module 'openai' {
	interface OpenAIStreamChunk {
		choices?: Array<{
			delta?: {
				content?: string | null;
				tool_calls?: Array<{
					index: number;
					id?: string;
					function?: { name?: string; arguments?: string };
				}>;
			};
			finish_reason?: string | null;
		}>;
		usage?: { prompt_tokens?: number; completion_tokens?: number } | null;
	}
	export default class OpenAI {
		constructor(opts?: Record<string, unknown>);
		chat: {
			completions: {
				create(
					params: Record<string, unknown>,
				): Promise<AsyncIterable<OpenAIStreamChunk>>;
			};
		};
	}
}
