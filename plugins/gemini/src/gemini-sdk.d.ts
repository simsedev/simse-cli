/** Stub for @google/genai (loaded at runtime by the Gemini plugin).
 *
 * The Deno runtime resolves the dynamic `import("@google/genai")` against the
 * installed package; this declaration only satisfies `tsc` for the surface
 * the plugin uses — the `GoogleGenAI` export and the streaming
 * `models.generateContentStream()` chunk shape. */
declare module '@google/genai' {
	interface GeminiStreamChunk {
		text?: string;
		functionCalls?: Array<{ name?: string; args?: Record<string, unknown> }>;
		usageMetadata?: {
			promptTokenCount?: number;
			candidatesTokenCount?: number;
		};
	}
	export class GoogleGenAI {
		constructor(opts?: Record<string, unknown>);
		models: {
			generateContentStream(
				params: Record<string, unknown>,
			): Promise<AsyncIterable<GeminiStreamChunk>>;
		};
	}
}
