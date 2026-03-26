// Copilot ACP Provider Plugin
// Uses @github/copilot-sdk for streaming chat completions.

import type { AcpPlugin, PluginMessage, PromptOptions, SimseHost } from '@simse/plugin-sdk';
import { registerPlugin } from '@simse/plugin-sdk';

declare const Simse: SimseHost;

interface ProviderConfig {
	cliUrl?: string;
	defaultModel?: string;
}

let client: any = null;
let defaultModel = "gpt-4.1";
const sessions: Map<string, any> = new Map();

const plugin: AcpPlugin = {
	auth: { type: "sdk_managed", description: "Handled by @github/copilot-sdk" },

	async initialize(config: ProviderConfig) {
		defaultModel = config.defaultModel ?? defaultModel;

		try {
			const { CopilotClient } = await import("@github/copilot-sdk");
			const clientOpts: Record<string, unknown> = {};
			if (config.cliUrl) clientOpts.cliUrl = config.cliUrl;
			client = new CopilotClient(clientOpts);
			await client.start();
		} catch (e) {
			throw new Error(
				`Failed to initialize Copilot SDK: ${e}. Ensure @github/copilot-sdk is installed.`,
			);
		}

		Simse.log(
			"info",
			`Copilot plugin initialized (model: ${defaultModel})`,
		);

		return {
			name: "copilot",
			version: "1.0.0",
			models: ["gpt-4.1", "gpt-5", "claude-sonnet-4"],
		};
	},

	async newSession(sessionId: string, options: Record<string, unknown>) {
		const model = (options.model as string) ?? defaultModel;

		const sessionOpts: Record<string, unknown> = {
			sessionId,
			model,
			streaming: true,
		};

		if (options.systemPrompt) {
			sessionOpts.systemMessage = {
				mode: "replace",
				content: options.systemPrompt,
			};
		}

		const session = await client.createSession(sessionOpts);
		sessions.set(sessionId, session);

		Simse.log("info", `Copilot session created: ${sessionId}`);
	},

	async prompt(
		sessionId: string,
		messages: PluginMessage[],
		options: PromptOptions,
	) {
		let session = sessions.get(sessionId);
		if (!session) {
			await plugin.newSession!(sessionId, options as Record<string, unknown>);
			session = sessions.get(sessionId);
		}

		const lastUserMsg = messages.filter((m) => m.role === "user").pop();
		if (!lastUserMsg) throw new Error("No user message found in messages");

		const done = new Promise<void>((resolve, reject) => {
			session.on("assistant.message_delta", (event: any) => {
				Simse.sendDelta(sessionId, event.data.deltaContent);
			});

			session.on("session.idle", () => {
				resolve();
			});

			session.on("error", (err: any) => {
				reject(new Error(`Copilot error: ${err}`));
			});
		});

		await session.send({ prompt: lastUserMsg.content });
		await done;

		Simse.sendComplete(sessionId, null);

		return { stopReason: "end_turn", usage: null };
	},

	async dispose() {
		for (const [_, session] of sessions) {
			try {
				await session.destroy();
			} catch {
				/* ignore */
			}
		}
		sessions.clear();

		if (client) {
			try {
				await client.stop();
			} catch {
				/* ignore */
			}
			client = null;
		}

		Simse.log("info", "Copilot plugin disposed");
	},
};

registerPlugin(plugin);
