import { spawn } from "node:child_process";
import { join } from "node:path";

const EVENTS = new Set(["session.created", "session.idle", "session.error"]);

function send(event, payload) {
  try {
    if (!process.env.TASKR_LAUNCH) return;
    const body = JSON.stringify(payload);
    const child = spawn(join(process.env.HOME, ".local/bin/taskr"), ["hook", "opencode", event], {
      detached: true,
      stdio: ["pipe", "ignore", "ignore"],
    });
    child.on("error", () => {});
    child.stdin.on("error", () => {});
    child.unref();
    child.stdin.end(body);
  } catch {}
}

export const TaskrHook = () => {
  try {
    if (!process.env.TASKR_LAUNCH) return {};
    return {
      "chat.message": (input, output) => {
        try {
          send("chat.message", { input, output });
        } catch {}
      },
      event: (data) => {
        try {
          const event = data?.event;
          if (event && EVENTS.has(event.type)) send(event.type, event);
        } catch {}
      },
    };
  } catch {
    return {};
  }
};

export default {
  id: "taskr.opencode",
  server: TaskrHook,
  async setup(ctx) {
    if (!process.env.TASKR_LAUNCH) return;
    try {
      const hooks = TaskrHook();
      await ctx.session.hook("prompt", ({ sessionID, prompt }) => {
        hooks["chat.message"]({ sessionID }, { parts: [{ type: "text", text: prompt.text }] });
      });
      const controller = new AbortController();
      // shortcut: no resubscribe after the stream ends; add retries if lifecycle events are lost.
      void (async () => {
        try {
          for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
            // Keep taskr's V1 event names/shapes for V2 lifecycle events.
            let type = event.type;
            if (type === "session.execution.failed") type = "session.error";
            if (type === "session.execution.interrupted" && event.data?.reason === "superseded") continue;
            if (type === "session.execution.succeeded" || type === "session.execution.interrupted") type = "session.idle";
            hooks.event({ event: { ...event, type, properties: event.data } });
          }
        } catch {}
      })();
      return () => controller.abort();
    } catch {}
  },
};
