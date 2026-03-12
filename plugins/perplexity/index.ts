// Perplexity MCP Plugin
// Provides a perplexity_search tool that calls the Perplexity Sonar API.

declare namespace Simse {
	function log(level: string, message: string): void;
}

interface PluginConfig {
	apiKey?: string;
	defaultModel?: string;
}

interface ToolDef {
	name: string;
	description: string;
	inputSchema: Record<string, unknown>;
}

interface ContentItem {
	type: string;
	text: string;
}

interface ToolResult {
	content: ContentItem[];
	isError?: boolean;
}

interface ResourceResult {
	contents: Array<{
		uri: string;
		text?: string;
		mimeType?: string;
	}>;
}

let apiKey = "";
let defaultModel = "sonar";

const SEARCH_TOOL: ToolDef = {
	name: "perplexity_search",
	description:
		"Search the web using Perplexity AI. Returns an AI-generated answer with citations.",
	inputSchema: {
		type: "object",
		properties: {
			query: {
				type: "string",
				description: "The search query to send to Perplexity.",
			},
			focus: {
				type: "string",
				enum: ["web", "academic", "news"],
				description:
					"Optional search focus area. Defaults to general web search.",
			},
		},
		required: ["query"],
	},
};

globalThis.__simsePlugin = {
	kind: "mcp",

	auth: {
		type: "api_key",
		name: "PERPLEXITY_API_KEY",
		description: "Perplexity API key for Sonar search",
		envVar: "PERPLEXITY_API_KEY",
		required: true,
	},

	async initialize(config: PluginConfig) {
		apiKey = config.apiKey ?? (config as any).__auth?.token ?? Deno.env.get("PERPLEXITY_API_KEY") ?? "";
		defaultModel = config.defaultModel ?? defaultModel;

		if (!apiKey) {
			throw new Error(
				"PERPLEXITY_API_KEY not set. Set it in plugin config or environment.",
			);
		}

		Simse.log(
			"info",
			`Perplexity plugin initialized (model: ${defaultModel})`,
		);

		return {
			name: "perplexity",
			version: "1.0.0",
			tools: [SEARCH_TOOL],
			resources: [],
		};
	},

	async callTool(
		name: string,
		args: Record<string, unknown>,
	): Promise<ToolResult> {
		if (name !== "perplexity_search") {
			return {
				content: [{ type: "text", text: `Unknown tool: ${name}` }],
				isError: true,
			};
		}

		const query = args.query as string;
		if (!query) {
			return {
				content: [
					{ type: "text", text: "Missing required parameter: query" },
				],
				isError: true,
			};
		}

		const focus = args.focus as string | undefined;

		try {
			const body: Record<string, unknown> = {
				model: defaultModel,
				messages: [
					{
						role: "system",
						content:
							"Be precise and concise. Provide factual answers with citations where possible.",
					},
					{
						role: "user",
						content: query,
					},
				],
			};

			if (focus) {
				body.search_focus = focus;
			}

			Simse.log("info", `Perplexity search: "${query}"`);

			const response = await fetch(
				"https://api.perplexity.ai/chat/completions",
				{
					method: "POST",
					headers: {
						authorization: `Bearer ${apiKey}`,
						"content-type": "application/json",
					},
					body: JSON.stringify(body),
				},
			);

			if (!response.ok) {
				const errorText = await response.text();
				Simse.log(
					"error",
					`Perplexity API error ${response.status}: ${errorText}`,
				);
				return {
					content: [
						{
							type: "text",
							text: `Perplexity API error ${response.status}: ${errorText}`,
						},
					],
					isError: true,
				};
			}

			const data = await response.json();
			const answer =
				data.choices?.[0]?.message?.content ?? "No answer returned.";
			const citations: string[] = data.citations ?? [];

			const content: ContentItem[] = [{ type: "text", text: answer }];

			if (citations.length > 0) {
				const citationText = citations
					.map((url: string, i: number) => `[${i + 1}] ${url}`)
					.join("\n");
				content.push({
					type: "text",
					text: `\nCitations:\n${citationText}`,
				});
			}

			return { content };
		} catch (err) {
			const message =
				err instanceof Error ? err.message : String(err);
			Simse.log("error", `Perplexity search failed: ${message}`);
			return {
				content: [
					{
						type: "text",
						text: `Perplexity search failed: ${message}`,
					},
				],
				isError: true,
			};
		}
	},

	async readResource(_uri: string): Promise<ResourceResult> {
		return { contents: [] };
	},

	async dispose() {
		Simse.log("info", "Perplexity plugin disposed");
	},
};
