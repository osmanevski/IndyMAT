const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

(async () => {
  const [base, token] = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8")).url.split("#");
  const request = (route, data, headers = {}) => fetch(base + "api/" + route, {
    method: data === undefined ? "GET" : "POST",
    headers: { "X-MF-Token": token, "X-MF-Language": "tr", ...data === undefined ? {} : { "Content-Type": "application/json" }, ...headers },
    body: data === undefined ? undefined : JSON.stringify(data)
  });
  const state = async () => (await request("state")).json();
  const idle = async () => {
    for (let index = 0; index < 600; index++) {
      const value = await state();
      if (value.status === "idle") return value;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error("idle timeout");
  };
  const payload = (document, extra = {}) => ({ code: document, mode: "code", argument: "", history: false,
    adapt_editor_literals: true,
    source_context: { document, span: [0, document.length], origin: "editor-selection", cursor: 0,
      path: path.resolve("workspace", "source_adapter_dirty.m"), revision: "http-snapshot", profile: "matlab" }, ...extra });
  let checks = 0;
  const equal = (actual, expected) => { assert.equal(actual, expected); checks++; };
  const run = async (data) => {
    const response = await request("execute", data);
    equal(response.status, 202);
    const accepted = await response.json();
    const result = await idle();
    equal(result.job, accepted.job);
    return { accepted, result };
  };
  const original = 'r = "ab" + "cd";';
  const { accepted: nativeReply, result: native } = await run({ code: original, mode: "code", argument: "", history: false });
  assert.deepEqual(Object.keys(nativeReply), ["job"]); checks++;
  equal(native.source_adapter, undefined);
  equal(native.variables.find((item) => item.name === "r").class, "double");
  equal(fs.readFileSync(path.join(".matlab-free/jobs", native.job, "code.m"), "utf8"), original);
  const { result: adapted } = await run(payload(original + ' total=sum([1 2;3 4],"all");'));
  equal(adapted.source_adapter.status, "adapted");
  equal(adapted.source_adapter.epoch, adapted.epoch);
  equal(adapted.variables.find((item) => item.name === "r").class, "string");
  equal(adapted.variables.find((item) => item.name === "total").preview, "10");
  equal(fs.existsSync(path.resolve("workspace", "source_adapter_dirty.m")), false);
  const { result: fallback } = await run(payload('r="x"; disp "hello"'));
  equal(fallback.source_adapter.status, "fallback");
  equal(fallback.source_adapter.diagnostics[0].code, "quoted-command");
  const failing = 'r="x"; missing_adapter_http;';
  const { result: failed } = await run(payload(failing));
  equal(failed.source_error_locations[0].column, failing.indexOf("missing_adapter_http") + 1);
  equal(failed.source_error_locations[0].job, failed.job);
  const before = failed.job;
  for (const data of [payload(original, { mode: "file", argument: "../outside.m" }),
    payload(original, { source_context: { ...payload(original).source_context, origin: "command-window" } }),
    payload(original, { source_context: { ...payload(original).source_context, span: [1,3] } }),
    payload(original, { source_context: { ...payload(original).source_context, path: "/etc/passwd" } }),
    payload(original, { source_context: { ...payload(original).source_context, profile: "unknown" } }),
    payload(original, { source_context: { ...payload(original).source_context, document: "😀", span: [1,2] } }),
    payload(original, { adapt_editor_literals: "true" })]) {
    const response = await request("execute", data);
    assert([400,403].includes(response.status), await response.text()); checks++;
    equal((await state()).job, before);
  }
  equal((await request("execute", payload(original), { Origin: "https://evil.example" })).status, 403);
  const section = '%% first\nr="a";\n%% next\nr="b";';
  const sectionPayload = payload(section);
  sectionPayload.code = section.slice(0, section.indexOf("\n%% next"));
  Object.assign(sectionPayload.source_context, { origin: "editor-section", span: [0, sectionPayload.code.length], cursor: 10 });
  equal((await run(sectionPayload)).result.source_adapter.status, "adapted");
  const { result: command } = await run({ code: 'command_native="x";', mode: "code", history: true });
  equal(command.source_adapter, undefined);
  equal(command.variables.find((item) => item.name === "command_native").class, "char");
  console.log(`HTTP SOURCE ADAPTER PASS: ${checks} assertions; native defaults, editor spans, flags, fallback, original columns, identity and security.`);
})().catch((error) => { console.error(error); process.exit(1); });
