// Test harness for the generated OpenCode plugin (crates/agent-adapters/tests/opencode_plugin.rs).
// Usage: node opencode_plugin_harness.mjs <plugin.mjs> <scenario>
// The plugin's `node:child_process` import is replaced by the test with `globalThis.__spawn`, so
// the child process is faked and every call can be inspected. Prints one JSON object.
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { pathToFileURL } from "node:url";

const [file, scenario] = process.argv.slice(2);
const calls = [];
const emitters = [];
let autoExit = true;
globalThis.__spawn = (exe, args, opts) => {
  const child = new EventEmitter();
  child.stdin = new PassThrough();
  child.unref = () => {};
  const call = { exe, args, opts, body: "" };
  child.stdin.on("data", (d) => (call.body += d));
  calls.push(call);
  emitters.push(child);
  if (autoExit) setImmediate(() => child.emit("exit", 0));
  return child;
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const mod = await import(pathToFileURL(file).href + "?" + scenario);
const plugin = mod.default;

// An async iterable the test pushes events into.
function source() {
  const queue = [];
  let wake;
  let closed = false;
  return {
    push: (e) => {
      queue.push(e);
      wake?.();
    },
    close: () => {
      closed = true;
      wake?.();
    },
    iterable: {
      async *[Symbol.asyncIterator]() {
        for (;;) {
          while (queue.length) yield queue.shift();
          if (closed) return;
          await new Promise((r) => (wake = r));
        }
      },
    },
  };
}

const src = source();
let subscribeOptions;
const ctx = {
  event: {
    subscribe: (options) => {
      subscribeOptions = options;
      return src.iterable;
    },
  },
};

const ev = (type, sessionID, extra = {}) => ({ type, data: { sessionID, ...extra } });
const settle = () => sleep(60);
const out = { scenario };

const keys = Object.keys(plugin).sort();
const returned = plugin.setup(ctx);
out.shape = {
  keys,
  id: plugin.id,
  setupReturnedPromise: returned instanceof Promise,
  cleanupIsFunction: typeof returned === "function",
  signalPassed: subscribeOptions?.signal instanceof AbortSignal,
};

if (scenario === "flow") {
  src.push(ev("session.created", "ses_a", { parentID: undefined, title: "SECRET TITLE" }));
  for (let i = 0; i < 6; i++)
    src.push(ev("session.tool.called", "ses_a", { input: { cmd: "SECRET_CMD" }, id: "t" }));
  src.push(ev("session.text.started", "ses_a"));
  src.push({ type: "session.text.delta", data: { sessionID: "ses_a", text: "SECRET_TEXT" } });
  src.push(ev("session.status", "ses_a", { status: { type: "busy" } }));
  src.push(ev("session.status", "ses_a", { status: { type: "idle" } }));
  await settle();
  out.afterBurst = calls.length;
  await sleep(1100);
  src.push(ev("session.execution.started", "ses_a"));
  src.push(
    ev("permission.asked", "ses_a", {
      action: "bash",
      resources: ["/SECRET/path"],
      message: "SECRET_MSG",
    }),
  );
  src.push(ev("permission.replied", "ses_a", { requestID: "r", reply: "always" }));
  src.push(ev("session.idle", "ses_a"));
  src.push(ev("session.idle", "ses_a"));
  src.push(ev("session.execution.succeeded", "ses_a"));
  // A failure is not overwritten by the idle that follows it.
  src.push(ev("session.execution.started", "ses_b"));
  src.push(ev("session.execution.failed", "ses_b", { error: { message: "SECRET_ERR" } }));
  src.push(ev("session.idle", "ses_b"));
  src.push(ev("session.deleted", "ses_a"));
  // Subagent sessions are not tracked as their own.
  src.push(ev("session.created", "ses_child", { parentID: "ses_a" }));
  src.push(ev("session.execution.started", "ses_child"));
  src.push(ev("permission.asked", "ses_child"));
  // Junk the plugin must survive.
  src.push(null);
  src.push({ type: 5 });
  src.push({ type: "session.created" });
  src.push({ type: "session.created", data: { sessionID: "x".repeat(500) } });
  src.push({
    type: "session.tool.called",
    get data() {
      throw new Error("boom");
    },
  });
  src.push(ev("session.execution.started", "ses_after_junk"));
  await settle();
} else if (scenario === "prompt") {
  const user = (text, type = "user", session = "ses_p") =>
    ev("session.inbox.enqueued", session, {
      inboxID: "i",
      item: { type, payload: { text, files: ["/SECRET/file"] } },
    });
  src.push(user("FIRST PROMPT"));
  src.push(user("synthetic", "synthetic"));
  // Only the first prompt of a session is sent; the next ones are not, even after a pause.
  await sleep(1100);
  src.push(user("SECOND PROMPT"));
  // Other sessions get their own first prompt, cut at 4000 code points.
  src.push(user("x".repeat(9000), "user", "ses_q"));
  // A surrogate pair at the cut is kept whole, never split.
  src.push(user("a".repeat(3999) + "\u{1F600}" + "tail", "user", "ses_r"));
  src.push(user("a".repeat(3998) + "\u{1F600}\u{1F600}" + "tail", "user", "ses_s"));
  await settle();
} else if (scenario === "flood") {
  autoExit = false;
  for (let i = 0; i < 40; i++) src.push(ev("permission.asked", "ses_" + i));
  await settle();
} else if (scenario === "dropped") {
  // Messages dropped because 8 forwarders are running are not remembered as sent: the same
  // event is sent again once there is room.
  autoExit = false;
  const prompt = (text) =>
    ev("session.inbox.enqueued", "ses_z", { item: { type: "user", payload: { text } } });
  for (let i = 0; i < 8; i++) src.push(ev("permission.asked", "ses_f" + i));
  await settle();
  src.push(ev("session.idle", "ses_x"));
  src.push(ev("session.execution.started", "ses_w"));
  src.push(ev("permission.asked", "ses_y"));
  src.push(prompt("PROMPT Z"));
  await settle();
  out.whileFull = calls.length;
  for (const c of emitters) c.emit("exit", 0);
  await settle();
  src.push(ev("session.idle", "ses_x"));
  src.push(ev("session.execution.started", "ses_w"));
  src.push(ev("permission.asked", "ses_y"));
  src.push(prompt("PROMPT Z AGAIN"));
  await settle();
  // And once sent, the prompt is not sent a third time.
  for (const c of emitters) c.emit("exit", 0);
  src.push(prompt("PROMPT Z THIRD"));
  await settle();
} else if (scenario === "spawn_throws") {
  globalThis.__spawn = () => {
    throw new Error("spawn failed");
  };
  src.push(ev("permission.asked", "ses_t"));
  src.push(ev("session.created", "ses_t"));
  await settle();
  out.survived = true;
} else if (scenario === "real") {
  // The unmodified plugin with the real child_process: a missing or wrong executable must not
  // throw or leave an unhandled error.
  src.push(ev("permission.asked", "ses_r"));
  src.push(ev("session.created", "ses_r"));
  await sleep(400);
  out.realOk = true;
} else if (scenario === "no_event_api") {
  // A host without ctx.event must not make setup throw.
  const r = plugin.setup({});
  out.noEventApi = typeof r === "function" || r === undefined;
}

src.close();
if (typeof returned === "function") returned();
out.aborted = subscribeOptions?.signal?.aborted === true;
out.calls = calls.map((c) => ({ exe: c.exe, args: c.args, opts: c.opts, body: c.body }));
process.stdout.write(JSON.stringify(out));
