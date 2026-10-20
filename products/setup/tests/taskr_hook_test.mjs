import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import opencodePlugin, { TaskrHook } from "../opencode/plugins/taskr-hook.js";

const savedEnv = Object.fromEntries(["HOME", "PATH", "TASKR_LAUNCH", "TASKR_RECORD_DIR"].map((key) => [key, process.env[key]]));
const root = await mkdtemp(join(tmpdir(), "taskr-hook-test-"));

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function recordFor(dir, event) {
  const file = join(dir, `${event}.json`);
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      return JSON.parse(await readFile(file, "utf8"));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      await delay(10);
    }
  }
  throw new Error(`taskr did not record ${event}`);
}

try {
  const home = join(root, "home");
  const bin = join(home, ".local/bin");
  const records = join(root, "records");
  await mkdir(bin, { recursive: true });
  await mkdir(records);
  await writeFile(
    join(bin, "taskr"),
    `#!/usr/bin/env node
const fs = require("node:fs");
const { join } = require("node:path");
const args = process.argv.slice(2);
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => { input += chunk; });
process.stdin.on("end", () => {
  fs.writeFileSync(join(process.env.TASKR_RECORD_DIR, args[2] + ".json"), JSON.stringify({ args, input }));
});
`,
    { mode: 0o755 },
  );

  process.env.HOME = home;
  process.env.PATH = `${bin}:${savedEnv.PATH ?? ""}`;
  process.env.TASKR_RECORD_DIR = records;
  delete process.env.TASKR_LAUNCH;
  assert.deepEqual(TaskrHook(), {});
  assert.equal(opencodePlugin.id, "taskr.opencode");
  assert.equal(opencodePlugin.server, TaskrHook);
  assert.equal(await opencodePlugin.setup({}), undefined);

  process.env.TASKR_LAUNCH = "test-launch";
  const hooks = TaskrHook();
  const chatInput = { sessionID: "session-1", prompt: "First taskr got 13459." };
  const chatOutput = { parts: [{ type: "text", text: "hello" }] };
  assert.equal(hooks["chat.message"](chatInput, chatOutput), undefined);
  const chat = await recordFor(records, "chat.message");
  assert.deepEqual(chat.args, ["hook", "opencode", "chat.message"]);
  assert.equal(chat.input, JSON.stringify({ input: chatInput, output: chatOutput }));

  for (const type of ["session.created", "session.idle", "session.error"]) {
    const event = { type, session: { id: "session-1" }, detail: "fixture" };
    assert.equal(hooks.event({ event }), undefined);
    const record = await recordFor(records, type);
    assert.deepEqual(record.args, ["hook", "opencode", type]);
    assert.equal(record.input, JSON.stringify(event));
  }

  assert.equal(hooks.event({ event: { type: "session.status", session: { id: "session-1" } } }), undefined);
  await delay(50);
  await assert.rejects(readFile(join(records, "session.status.json"), "utf8"), { code: "ENOENT" });

  await rm(join(records, "chat.message.json"));
  await rm(join(records, "session.created.json"));
  const circular = { parts: [] };
  circular.self = circular;
  assert.doesNotThrow(() => hooks["chat.message"](chatInput, circular));
  assert.doesNotThrow(() => hooks.event({ event: { type: "session.created", n: 1n } }));
  await delay(100);
  await assert.rejects(readFile(join(records, "chat.message.json"), "utf8"), { code: "ENOENT" });
  await assert.rejects(readFile(join(records, "session.created.json"), "utf8"), { code: "ENOENT" });

  // Exercise the V2 entrypoint, including payload translation and cleanup.
  for (const [source, target, reason] of [
    ["session.created", "session.created"],
    ["session.execution.succeeded", "session.idle"],
    ["session.execution.failed", "session.error"],
    ["session.execution.interrupted", "session.idle", "user"],
    ["session.execution.interrupted", null, "superseded"],
    ["session.status", null],
  ]) {
    const event = { id: "v2-event", type: source, data: { sessionID: "v2-session", reason, error: { code: "fixture" } } };
    for (const type of ["chat.message", "session.created", "session.idle", "session.error"]) {
      await rm(join(records, `${type}.json`), { force: true });
    }
    let subscriptionSignal;
    let v2Prompt;
    const cleanup = await opencodePlugin.setup({
      session: { hook: async (name, callback) => {
        assert.equal(name, "prompt");
        v2Prompt = callback;
      } },
      event: { subscribe: async function* ({ signal }) {
        subscriptionSignal = signal;
        yield event;
        await new Promise((resolve) => signal.addEventListener("abort", resolve, { once: true }));
      } },
    });
    v2Prompt({ sessionID: "v2-session", prompt: { text: "First taskr got 42." } });
    assert.deepEqual(JSON.parse((await recordFor(records, "chat.message")).input), {
      input: { sessionID: "v2-session" },
      output: { parts: [{ type: "text", text: "First taskr got 42." }] },
    });
    if (target) {
      const record = await recordFor(records, target);
      assert.deepEqual(record.args, ["hook", "opencode", target]);
      assert.equal(record.input, JSON.stringify({ ...event, type: target, properties: event.data }));
    } else {
      for (const type of ["session.created", "session.idle", "session.error", "session.status"]) {
        await assert.rejects(readFile(join(records, `${type}.json`), "utf8"), { code: "ENOENT" });
      }
    }
    assert.equal(subscriptionSignal.aborted, false);
    cleanup();
    assert.equal(subscriptionSignal.aborted, true);
  }

  await writeFile(join(bin, "taskr"), "#!/bin/sh\nexec sleep 2\n", { mode: 0o755 });
  const calls = [
    () => hooks["chat.message"](chatInput, chatOutput),
    ...["session.created", "session.idle", "session.error"].map((type) => () => hooks.event({ event: { type } })),
  ];
  for (const call of calls) {
    const start = performance.now();
    assert.equal(call(), undefined);
    assert.ok(performance.now() - start < 100, "hook call blocked on taskr");
  }

  process.env.HOME = join(root, "missing-home");
  assert.doesNotThrow(() => hooks.event({ event: { type: "session.idle" } }));
  await delay(50);
} finally {
  for (const [key, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
  await rm(root, { recursive: true, force: true });
}

console.log("taskr OpenCode hooks passed");
