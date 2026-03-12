(() => {
  var __create = Object.create;
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getProtoOf = Object.getPrototypeOf;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
    // If the importer is in node compatibility mode or this is not an ESM
    // file that has been converted to a CommonJS file using a Babel-
    // compatible transform (i.e. "__esModule" has not been set), then set
    // "default" to the CommonJS "module.exports" for node compatibility.
    isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
    mod
  ));
  let client = null;
  let defaultModel = "gpt-4.1";
  const sessions = /* @__PURE__ */ new Map();
  globalThis.__simsePlugin = {
    auth: { type: "sdk_managed", description: "Handled by @github/copilot-sdk" },
    async initialize(config) {
      defaultModel = config.defaultModel ?? defaultModel;
      try {
        const { CopilotClient } = await import("@github/copilot-sdk");
        const clientOpts = {};
        if (config.cliUrl) clientOpts.cliUrl = config.cliUrl;
        client = new CopilotClient(clientOpts);
        await client.start();
      } catch (e) {
        throw new Error(
          `Failed to initialize Copilot SDK: ${e}. Ensure @github/copilot-sdk is installed.`
        );
      }
      Simse.log(
        "info",
        `Copilot plugin initialized (model: ${defaultModel})`
      );
      return {
        name: "copilot",
        version: "1.0.0",
        models: ["gpt-4.1", "gpt-5", "claude-sonnet-4"]
      };
    },
    async newSession(sessionId, options) {
      const model = options.model ?? defaultModel;
      const sessionOpts = {
        sessionId,
        model,
        streaming: true
      };
      if (options.systemPrompt) {
        sessionOpts.systemMessage = {
          mode: "replace",
          content: options.systemPrompt
        };
      }
      const session = await client.createSession(sessionOpts);
      sessions.set(sessionId, session);
      Simse.log("info", `Copilot session created: ${sessionId}`);
    },
    async prompt(sessionId, messages, options) {
      let session = sessions.get(sessionId);
      if (!session) {
        await globalThis.__simsePlugin.newSession(sessionId, options);
        session = sessions.get(sessionId);
      }
      const lastUserMsg = messages.filter((m) => m.role === "user").pop();
      if (!lastUserMsg) throw new Error("No user message found in messages");
      const done = new Promise((resolve, reject) => {
        session.on("assistant.message_delta", (event) => {
          Simse.sendDelta(sessionId, event.data.deltaContent);
        });
        session.on("session.idle", () => {
          resolve();
        });
        session.on("error", (err) => {
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
        }
      }
      sessions.clear();
      if (client) {
        try {
          await client.stop();
        } catch {
        }
        client = null;
      }
      Simse.log("info", "Copilot plugin disposed");
    }
  };
})();
