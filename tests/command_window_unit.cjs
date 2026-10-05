const assert = require("node:assert/strict");
const { commandIsIncomplete, commandLexicalState, historyRecall, parseErrorLocations, trailingContinuation, decorationSegments } = require("../frontend/command_window_utils.cjs");

// Review 4: indexing commas/semicolons and end tokens are not block closers.
for (const expression of ["x=A(:, end)", "x=A(1,\n end)", "x=C{1, end}", "x=[A(1,end); A(2,end)]"]) {
  const code = `if true\n A=[1 2];\n ${expression}`;
  assert.equal(commandIsIncomplete(code), true, code);
  assert.equal(commandIsIncomplete(code + "\nend"), false, code + "\nend");
}
assert.equal(commandIsIncomplete("if(true), x=A(:,end); endif"), false);

for (const source of ["if ready", "for k=1:3\ndisp(k)", "while true", "function y=f(x)\ny=x", "switch x\ncase 1", "try\ndisp(1)", "value = [1, 2", "call({1, 2}", "value = 1 + ..."]) {
  assert.equal(commandIsIncomplete(source), true, source);
}
for (const source of ["if ready, value=1; end", "for k=1:3, disp(k); endfor", "value = [1, 2]", "disp('if [ ...')", "% if ready\nvalue=1", "%{\nif hidden\n%}\nvalue=1", "A(end)", "value = 1; % ..."]) {
  assert.equal(commandIsIncomplete(source), false, source);
}
assert.deepEqual(commandLexicalState("if x\nfor k=1:2\nend\nend"), { blocks: 0, brackets: 0, continuation: false });
assert.equal(trailingContinuation("x = '...'; % comment"), false);
assert.equal(trailingContinuation("x = 1 + ... % açıklama"), true);

const recall = historyRecall(["plot(x)", "disp(x)", "plot(y)"], "plo", "plo");
assert.deepEqual(recall.matches, ["plot(x)", "plot(y)"]);
assert.equal(recall.draft, "plo");

const message = "parse error near line 17, column 4 of file /tmp/work/demo.m\nerror: called from\n    helper at line 8 column 2\nother.m:23:5";
assert.deepEqual(parseErrorLocations(message).map(({ path, line }) => ({ path, line })), [
  { path: "/tmp/work/demo.m", line: 17 },
  { path: "helper", line: 8 },
  { path: "other.m", line: 23 }
]);
assert.deepEqual(parseErrorLocations("error: plain failure"), []);

const captured = require("./fixtures/command_window_errors.json");
for (const fixture of captured.cases) {
  const actual = parseErrorLocations(fixture.error);
  assert.deepEqual(actual.map(({ path, line }) => ({ path, line })), fixture.locations, fixture.name);
  for (const location of actual) {
    assert(fixture.error.slice(location.start, location.end).includes(location.path), fixture.name + " preserves the clickable text range");
  }
}
assert.deepEqual(parseErrorLocations("syntax error near line 2, column 11 in file /work/folder with spaces/demo.m").map(({ path, line }) => ({ path, line })), [{ path: "/work/folder with spaces/demo.m", line: 2 }]);
assert.deepEqual(parseErrorLocations("'missing' undefined near line 2, column 9"), [], "a line number without a file is not a location");
assert.deepEqual(parseErrorLocations("clock 12:30\nhttp://localhost:8080\n  run:0"), [], "unrelated colon text is not a frame");

console.log("COMMAND WINDOW UNIT PASS: multiline completeness, comments/strings, prefix recall and error locations.");

// Exercise the product renderer with real captured response text; only DOM and
// read-only location resolution are doubled. No numerical results are mocked.
const fs = require("node:fs");
const vm = require("node:vm");
class Node {
  constructor(tag, text = "") {
    this.tag = tag;
    this.text = text;
    this.children = [];
    this.dataset = {};
    this.className = "";
    this.isConnected = true;
    this.value = "";
    this.style = {};
    this.scrollHeight = 22;
    this.classList = { toggle() {} };
  }
  get textContent() { return this.text + this.children.map((node) => node.textContent).join(""); }
  set textContent(text) { this.text = text; this.children = []; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.text = ""; this.children = nodes; }
}
const controls = new Map(["#console-search", "#console-match-count", "#console-search-previous", "#console-search-next"].map((id) => [id, new Node("input")]));
let outputs = [], locationRequests = 0;
const registry = {
  on() {},
  $: (id) => controls.get(id),
  el: (tag, className) => Object.assign(new Node(tag), { className }),
  api: async (route, { path, line }) => {
    assert.equal(route, "error-location");
    locationRequests++;
    if (path === "run") throw Error("not found");
    return { path: path.startsWith("<workspace>") ? path : "/work/" + path.split(">")[0] + ".m", line };
  }
};
const document = {
  querySelectorAll: () => outputs,
  createDocumentFragment: () => new Node("fragment"),
  createTextNode: (text) => new Node("text", text)
};
const source = fs.readFileSync("frontend/command_window.js", "utf8").replace(/^import .*;\n/gm, "");
let observeMutation;
class MutationObserver {
  constructor(callback) { observeMutation = callback; }
  observe() {}
  disconnect() {}
}
vm.runInNewContext(source + "\nregistry.observeForTest = observeConsoleOutput;", { registry, document, MutationObserver, setTimeout, clearTimeout, TextEncoder, TextDecoder, t: (source, params = {}) => source.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (_, name) => params[name] ?? `{${name}}`), onLanguageChange: () => () => {}, shared: {}, commandWindowUtils: require("../frontend/command_window_utils.cjs") });
registry.observeForTest();
const links = (node) => [node, ...node.children.flatMap(links)].filter((child) => child.className === "console-location");
const descendants = (node) => [node, ...node.children.flatMap(descendants)];
const settle = (delay = 120) => new Promise((resolve) => setTimeout(resolve, delay));
(async () => {
  for (const fixture of captured.cases) {
    // poll.js writes state.output and state.error into distinct pre elements.
    const out = new Node("pre", fixture.output), err = new Node("pre", fixture.error);
    err.className = "console-output console-error";
    out.className = "console-output";
    outputs = [out, err];
    registry.renderOutputSearch();
    registry.renderOutputSearch();
    await settle();
    assert.equal(err.textContent, fixture.error, fixture.name + " changed rendered error text");
    assert.equal(links(err).length, fixture.locations.filter((item) => item.path !== "run").length, fixture.name);
    for (const link of links(err)) assert.equal(link.tag, "button");
    const previous = locationRequests;
    const previousChildren = err.children;
    registry.renderOutputSearch();
    await settle();
    assert.equal(locationRequests, previous, "unchanged output must not resolve locations repeatedly");
    assert.equal(err.children, previousChildren, "unchanged output must retain its decoration DOM");
    if (fixture.name === "cw_capture_function") {
      controls.get("#console-search").value = fixture.name;
      registry.renderOutputSearch();
      assert.equal(controls.get("#console-match-count").textContent, "1 / 2");
      assert.equal(descendants(err).filter((node) => node.tag === "mark").length, 2);
      assert.equal(err.textContent, fixture.error);
      controls.get("#console-search").value = "";
    }
  }
  // Raw stderr is collected in state.output. Its location must also be linked.
  outputs = [new Node("pre", "error: called from\n    cw_capture_function at line 2 column 9")];
  registry.renderOutputSearch();
  await settle();
  assert.equal(links(outputs[0]).length, 1);
  // A delayed result for an old text revision must never resurrect old links.
  let complete;
  registry.api = () => new Promise((resolve) => { complete = resolve; });
  outputs = [new Node("pre", captured.cases[0].error.replaceAll("cw_capture_function", "late_fixture"))];
  registry.renderOutputSearch();
  outputs[0].textContent = "new output";
  observeMutation();
  complete({ path: "/work/cw_capture_function.m", line: 2 });
  await settle();
  assert.equal(outputs[0].textContent, "new output");
  assert.equal(links(outputs[0]).length, 0);
  console.log("COMMAND WINDOW RENDER PASS: caught errors/stdout, resolution, search coexistence and stale-result guard.");

  controls.set("#command", new Node("textarea"));
  controls.set("#command-form", new Node("form"));
  registry.setupCommandWindow();
  outputs = [new Node("pre", "a".repeat(1_000_000))];
  for (let i = 0; i < 30; i++) {
    controls.get("#console-search").value = i === 29 ? "a" : "absent";
    controls.get("#console-search").oninput();
  }
  assert.equal(outputs[0].children.length, 0, "input handler must schedule, not synchronously decorate");
  await settle(150);
  assert.equal(controls.get("#console-match-count").textContent, "1 / 1000+");

  // Review 3: a full MB of matches creates bounded DOM; a miss also finishes.
  outputs = [new Node("pre", "a".repeat(1_000_000))];
  controls.get("#console-search").value = "a";
  let started = performance.now();
  registry.renderOutputSearch();
  const elapsed = performance.now() - started;
  assert.equal(controls.get("#console-match-count").textContent, "1 / 1000+");
  assert.equal(descendants(outputs[0]).filter((node) => node.tag === "mark").length, 1000);
  assert(descendants(outputs[0]).length < 2100);
  controls.get("#console-search").value = "absent";
  registry.renderOutputSearch();
  assert.equal(controls.get("#console-match-count").textContent, "0");
  assert.equal(outputs[0].textContent.length, 1_000_000);
  const ranges = Array.from({ length: 20000 }, (_, index) => ({ start: index * 2, end: index * 2 + 1 }));
  let accesses = 0;
  const tracked = new Proxy(ranges, { get(target, key) { if (/^\d+$/.test(String(key))) accesses++; return target[key]; } });
  assert.equal(decorationSegments(40000, tracked, []).length, 40000);
  assert(accesses < 200000, `range sweep did ${accesses} accesses`);
  assert.equal(parseErrorLocations("a".repeat(1_000_000)).length, 0);
  assert.equal(parseErrorLocations("file.m:1\n".repeat(100000)).length, 64);

  // More candidates across many entries still share one quota and four slots.
  let requests = 0, active = 0, peak = 0;
  registry.api = async (route, payload) => {
    requests++;
    active++;
    peak = Math.max(peak, active);
    await settle(1);
    active--;
    return { ...payload, path: "/work/" + payload.path };
  };
  controls.get("#console-search").value = "";
  outputs = Array.from({ length: 100 }, (_, index) => new Node("pre", Array.from({ length: 100 }, (_, inner) => `budget_${index}_${inner}.m:1\n`).join("")));
  registry.renderOutputSearch();
  await settle(1300);
  assert.equal(requests, 64, "one 64-candidate quota across all entries");
  assert.equal(peak, 4, "four requests at a time");
  assert(outputs.reduce((sum, node) => sum + links(node).length, 0) <= 64);

  // Console retention is a global UTF-8 byte budget, not 100 large entries.
  const root = { children: [] };
  controls.set("#console", root);
  outputs = [];
  for (let index = 0; index < 3; index++) {
    const out = new Node("pre", "ğ".repeat(250000));
    const entry = {
      get textContent() { return out.textContent; },
      querySelectorAll: () => [out],
      remove() { root.children.splice(root.children.indexOf(this), 1); outputs.splice(outputs.indexOf(out), 1); out.isConnected = false; }
    };
    root.children.push(entry);
    outputs.push(out);
  }
  registry.renderOutputSearch();
  assert.equal(root.children.length, 2);
  assert.equal(Buffer.byteLength(root.children.map((entry) => entry.textContent).join("")), 1_000_000);
  outputs[1].textContent = "ğ".repeat(600000);
  observeMutation();
  await settle();
  assert.equal(root.children.length, 1);
  assert(Buffer.byteLength(root.children[0].textContent) <= 1_000_000);
  assert(!root.children[0].textContent.includes("�"));
  console.log(`COMMAND WINDOW BOUNDS PASS: 1 MB renderer ${elapsed.toFixed(2)} ms, 1000 highlights, ${requests} requests, peak ${peak}, UTF-8 retention and linear sweep.`);
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
