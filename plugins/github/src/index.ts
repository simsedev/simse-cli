// GitHub MCP Plugin
// Provides GitHub REST API tools and resources via the MCP interface.

import type { McpPlugin, McpToolResult, McpResourceResult, SimseHost } from '@simse/plugin-sdk';


declare const Simse: SimseHost;
declare const Deno: { env: { get(key: string): string | undefined; set(key: string, value: string): void } };

interface PluginConfig {
	token?: string;
	baseUrl?: string;
}

let token = "";
let baseUrl = "https://api.github.com";

function headers(): Record<string, string> {
	const h: Record<string, string> = {
		"accept": "application/vnd.github+json",
		"x-github-api-version": "2022-11-28",
	};
	if (token) h["authorization"] = `Bearer ${token}`;
	return h;
}

async function ghFetch(
	path: string,
	params?: Record<string, string | number | undefined>,
): Promise<unknown> {
	const url = new URL(path, baseUrl);
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			if (v !== undefined) url.searchParams.set(k, String(v));
		}
	}

	const response = await fetch(url.toString(), { headers: headers() });

	if (!response.ok) {
		const body = await response.text();
		throw new Error(`GitHub API error ${response.status}: ${body}`);
	}

	return response.json();
}

function textResult(data: unknown): McpToolResult {
	return {
		content: [{ type: "text" as const, text: JSON.stringify(data, null, 2) }],
	};
}

function errorResult(msg: string): McpToolResult {
	return {
		content: [{ type: "text" as const, text: msg }],
		isError: true,
	};
}

(globalThis as any).__simsePlugin = ({
	kind: "mcp",

	auth: [
		{
			type: "oauth",
			provider: "github",
			clientId: "Iv1.xxxxxxxx",
			scopes: ["repo", "read:org"],
			deviceAuthUrl: "https://github.com/login/device/code",
			tokenUrl: "https://github.com/login/oauth/access_token",
			envVar: "GITHUB_TOKEN",
		},
		{
			type: "api_key",
			name: "GITHUB_TOKEN",
			description: "GitHub personal access token (fallback if OAuth unavailable)",
			envVar: "GITHUB_TOKEN",
			required: false,
		},
	],

	async initialize(config: PluginConfig) {
		token = config.token ?? (config as any).__auth?.token ?? Deno.env.get("GITHUB_TOKEN") ?? "";
		baseUrl = config.baseUrl ?? baseUrl;

		if (!token) {
			Simse.log(
				"warn",
				"GITHUB_TOKEN not set. Requests will be unauthenticated with lower rate limits.",
			);
		}

		Simse.log("info", `GitHub MCP plugin initialized (baseUrl: ${baseUrl})`);

		return {
			name: "github",
			version: "1.0.0",
			tools: [
				{
					name: "github_search_repos",
					description: "Search GitHub repositories",
					inputSchema: {
						type: "object",
						properties: {
							query: { type: "string", description: "Search query" },
							sort: {
								type: "string",
								enum: ["stars", "forks", "updated", "help-wanted-issues"],
								description: "Sort field",
							},
							per_page: {
								type: "number",
								description: "Results per page (max 100)",
							},
						},
						required: ["query"],
					},
				},
				{
					name: "github_get_repo",
					description: "Get repository details",
					inputSchema: {
						type: "object",
						properties: {
							owner: { type: "string", description: "Repository owner" },
							repo: { type: "string", description: "Repository name" },
						},
						required: ["owner", "repo"],
					},
				},
				{
					name: "github_list_issues",
					description: "List issues for a repository",
					inputSchema: {
						type: "object",
						properties: {
							owner: { type: "string", description: "Repository owner" },
							repo: { type: "string", description: "Repository name" },
							state: {
								type: "string",
								enum: ["open", "closed", "all"],
								description: "Issue state filter",
							},
							per_page: {
								type: "number",
								description: "Results per page (max 100)",
							},
						},
						required: ["owner", "repo"],
					},
				},
				{
					name: "github_get_issue",
					description: "Get a single issue",
					inputSchema: {
						type: "object",
						properties: {
							owner: { type: "string", description: "Repository owner" },
							repo: { type: "string", description: "Repository name" },
							issue_number: { type: "number", description: "Issue number" },
						},
						required: ["owner", "repo", "issue_number"],
					},
				},
				{
					name: "github_search_code",
					description: "Search code across GitHub",
					inputSchema: {
						type: "object",
						properties: {
							query: { type: "string", description: "Search query" },
							per_page: {
								type: "number",
								description: "Results per page (max 100)",
							},
						},
						required: ["query"],
					},
				},
				{
					name: "github_list_pulls",
					description: "List pull requests for a repository",
					inputSchema: {
						type: "object",
						properties: {
							owner: { type: "string", description: "Repository owner" },
							repo: { type: "string", description: "Repository name" },
							state: {
								type: "string",
								enum: ["open", "closed", "all"],
								description: "Pull request state filter",
							},
							per_page: {
								type: "number",
								description: "Results per page (max 100)",
							},
						},
						required: ["owner", "repo"],
					},
				},
				{
					name: "github_get_file",
					description: "Get file contents from a repository",
					inputSchema: {
						type: "object",
						properties: {
							owner: { type: "string", description: "Repository owner" },
							repo: { type: "string", description: "Repository name" },
							path: { type: "string", description: "File path" },
							ref: {
								type: "string",
								description: "Git ref (branch, tag, or SHA)",
							},
						},
						required: ["owner", "repo", "path"],
					},
				},
			],
			resources: [
				{
					uri: "github://repos/{owner}/{repo}/readme",
					name: "Repository README",
					description: "Get the README file for a GitHub repository",
					mimeType: "text/markdown",
				},
			],
		};
	},

	tools() {
		// Tools are declared in initialize; this satisfies the McpPlugin interface.
		return [];
	},

	async callTool(name: string, args: Record<string, unknown>) {
		try {
			switch (name) {
				case "github_search_repos": {
					const data = await ghFetch("/search/repositories", {
						q: args.query as string,
						sort: args.sort as string | undefined,
						per_page: args.per_page as number | undefined,
					});
					return textResult(data);
				}

				case "github_get_repo": {
					const data = await ghFetch(
						`/repos/${args.owner}/${args.repo}`,
					);
					return textResult(data);
				}

				case "github_list_issues": {
					const data = await ghFetch(
						`/repos/${args.owner}/${args.repo}/issues`,
						{
							state: args.state as string | undefined,
							per_page: args.per_page as number | undefined,
						},
					);
					return textResult(data);
				}

				case "github_get_issue": {
					const data = await ghFetch(
						`/repos/${args.owner}/${args.repo}/issues/${args.issue_number}`,
					);
					return textResult(data);
				}

				case "github_search_code": {
					const data = await ghFetch("/search/code", {
						q: args.query as string,
						per_page: args.per_page as number | undefined,
					});
					return textResult(data);
				}

				case "github_list_pulls": {
					const data = await ghFetch(
						`/repos/${args.owner}/${args.repo}/pulls`,
						{
							state: args.state as string | undefined,
							per_page: args.per_page as number | undefined,
						},
					);
					return textResult(data);
				}

				case "github_get_file": {
					const data = (await ghFetch(
						`/repos/${args.owner}/${args.repo}/contents/${args.path}`,
						{ ref: args.ref as string | undefined },
					)) as Record<string, unknown>;

					if (data.encoding === "base64" && typeof data.content === "string") {
						const decoded = atob(
							(data.content as string).replace(/\n/g, ""),
						);
						return textResult({ ...data, content: decoded, encoding: "utf-8" });
					}

					return textResult(data);
				}

				default:
					return errorResult(`Unknown tool: ${name}`);
			}
		} catch (e) {
			return errorResult(`${e}`);
		}
	},

	async readResource(uri: string): Promise<McpResourceResult> {
		const match = uri.match(/^github:\/\/repos\/([^/]+)\/([^/]+)\/readme$/);
		if (!match) {
			throw new Error(`Unknown resource URI: ${uri}`);
		}

		const [, owner, repo] = match;

		try {
			const data = (await ghFetch(
				`/repos/${owner}/${repo}/readme`,
			)) as Record<string, unknown>;

			let content = "";
			if (data.encoding === "base64" && typeof data.content === "string") {
				content = atob((data.content as string).replace(/\n/g, ""));
			} else if (typeof data.content === "string") {
				content = data.content;
			}

			return {
				contents: [
					{
						uri,
						text: content,
						mimeType: "text/markdown",
					},
				],
			};
		} catch (e) {
			throw new Error(`Failed to read README for ${owner}/${repo}: ${e}`);
		}
	},

	async dispose() {
		Simse.log("info", "GitHub MCP plugin disposed");
	},
} satisfies McpPlugin);
