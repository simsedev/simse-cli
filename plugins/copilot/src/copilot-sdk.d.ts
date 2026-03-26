/** Stub for @github/copilot-sdk (loaded at runtime by the Copilot plugin). */
declare module '@github/copilot-sdk' {
	export class CopilotClient {
		constructor(opts?: Record<string, unknown>);
		start(): Promise<void>;
		stop(): Promise<void>;
		createSession(opts: Record<string, unknown>): Promise<any>;
	}
}
