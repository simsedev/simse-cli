(() => {
  let baseUrl = "http://localhost:11434";
  let defaultModel = "llama3.1";
  globalThis.__simsePlugin = {
    auth: { type: "none" },
    async initialize(config) {
      baseUrl = config.baseUrl ?? baseUrl;
      defaultModel = config.defaultModel ?? defaultModel;
      try {
        const healthResp = await fetch(`${baseUrl}/api/tags`);
        if (!healthResp.ok) throw new Error(`HTTP ${healthResp.status}`);
        const data = await healthResp.json();
        const models = (data.models ?? []).map((m) => m.name);
        Simse.log("info", `Ollama plugin initialized (${models.length} models available)`);
        return {
          name: "ollama",
          version: "1.0.0",
          models: models.length > 0 ? models : [defaultModel]
        };
      } catch (e) {
        Simse.log("warn", `Ollama not reachable at ${baseUrl}: ${e}. Will retry on first prompt.`);
        return {
          name: "ollama",
          version: "1.0.0",
          models: [defaultModel]
        };
      }
    },
    async newSession(_sessionId, _options) {
    },
    async prompt(sessionId, messages, options) {
      const model = options.model ?? defaultModel;
      const apiMessages = [];
      if (options.systemPrompt) {
        apiMessages.push({ role: "system", content: options.systemPrompt });
      }
      for (const m of messages) {
        apiMessages.push({ role: m.role, content: m.content });
      }
      const body = {
        model,
        messages: apiMessages,
        stream: true
      };
      if (options.temperature !== void 0) body.temperature = options.temperature;
      if (options.topP !== void 0) body.top_p = options.topP;
      if (options.maxTokens !== void 0) body.max_tokens = options.maxTokens;
      const response = await fetch(`${baseUrl}/v1/chat/completions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body)
      });
      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Ollama API error ${response.status}: ${errorText}`);
      }
      const reader = response.body.getReader();
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
          if (!trimmed || trimmed === "data: [DONE]") continue;
          const jsonStr = trimmed.startsWith("data: ") ? trimmed.slice(6) : trimmed;
          try {
            const data = JSON.parse(jsonStr);
            const choice = data.choices?.[0];
            if (choice?.delta?.content) {
              Simse.sendDelta(sessionId, choice.delta.content);
            }
            if (choice?.finish_reason) {
              stopReason = choice.finish_reason === "stop" ? "end_turn" : "max_tokens";
            }
            if (data.usage) {
              promptTokens = data.usage.prompt_tokens ?? 0;
              completionTokens = data.usage.completion_tokens ?? 0;
            }
          } catch {
          }
        }
      }
      const usage = promptTokens > 0 || completionTokens > 0 ? { promptTokens, completionTokens } : null;
      Simse.sendComplete(sessionId, usage);
      return { stopReason, usage };
    },
    async dispose() {
      Simse.log("info", "Ollama plugin disposed");
    }
  };
})();
