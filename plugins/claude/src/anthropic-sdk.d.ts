/** Stub for @anthropic-ai/sdk (loaded at runtime by the Claude plugin).
 *
 * The Deno runtime loads the plugin as a classic script and resolves the
 * dynamic `import("@anthropic-ai/sdk")` against the installed package. This
 * declaration only needs to satisfy `tsc` for the surface the plugin uses:
 * the default `Anthropic` export, `messages.stream()`, and the returned
 * stream's `on("text", ...)` / `finalMessage()` helpers. */
declare module '@anthropic-ai/sdk' {
	interface MessageStream {
		on(event: 'text', cb: (delta: string) => void): MessageStream;
		finalMessage(): Promise<{
			content: Array<Record<string, unknown>>;
			stop_reason?: string | null;
			usage?: { input_tokens?: number; output_tokens?: number };
		}>;
	}
	export default class Anthropic {
		constructor(opts?: Record<string, unknown>);
		messages: {
			stream(params: Record<string, unknown>): MessageStream;
		};
	}
}
