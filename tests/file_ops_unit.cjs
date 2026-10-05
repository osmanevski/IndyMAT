const assert = require("node:assert/strict");
const esbuild = require("esbuild");
const path = require("node:path");

const bundle = esbuild.buildSync({
  stdin: {
    contents: 'import "./frontend/debugger.js"; export * from "./frontend/file_ops.js"; export {default as shared} from "./frontend/state.js"; export {default as registry} from "./frontend/registry.js";',
    resolveDir: path.resolve(__dirname, ".."),
  },
  bundle: true,
  write: false,
  platform: "node",
  format: "cjs",
});
const output = { exports: {} };
new Function("module", "exports", "require", bundle.outputFiles[0].text)(output, output.exports, require);
const { shared, registry, relocateOpenPaths, relocateFolderHistory, expectedFileHashes, trashOpenPaths } = output.exports;
for (const name of ["renderTabs", "persistDrafts"]) registry[name] = () => {};
let debugPanelRenders = 0, debugEditorRefreshes = 0;
registry.renderDebuggerPanel = () => debugPanelRenders++;
registry.refreshDebugEditor = () => debugEditorRefreshes++;

// Finding 1: disk hash B can never authorize saving editor contents based on A.
for (const dirty of [false, true]) {
  shared.tabs = [{ path: "/work/a.m", content: "A", saved: "A", hash: "hash-A", dirty }];
  shared.folderHistory = ["/work"];
  shared.folderIndex = 0;
  assert.deepEqual(expectedFileHashes("/work/a.m"), [{ path: "/work/a.m", hash: "hash-A" }]);
  relocateOpenPaths("/work/a.m", "/work/b.m", "hash-B");
  assert.equal(shared.tabs[0].hash, "hash-A");
  assert.equal(shared.tabs[0].content, "A");
  assert.equal(shared.tabs[0].dirty, dirty);
  assert.equal(shared.tabs[0].path, "/work/b.m");
}

// Rich debugger metadata survives a file rename and folder move; trash drops
// only the affected prefix. The debugger registry owns this transformation.
shared.engine = { status: "idle" };
debugPanelRenders = debugEditorRefreshes = 0;
shared.breakpoints = new Map([
  ["/work/old/a.m", new Map([[2, { enabled: true, condition: "value == 2" }], [4, { enabled: false, condition: "value > 9" }]])],
  ["/work/old/sub/b.m", new Map([[3, { enabled: true, condition: "iteration == 3" }]])],
  ["/work/older/c.m", new Map([[1, { enabled: true, condition: "" }]])],
]);
shared.tabs = [];
relocateOpenPaths("/work/old/a.m", "/work/old/renamed.m");
assert.deepEqual([...shared.breakpoints.get("/work/old/renamed.m")], [[2, { enabled: true, condition: "value == 2" }], [4, { enabled: false, condition: "value > 9" }]]);
relocateOpenPaths("/work/old", "/work/new");
assert.equal(shared.breakpoints.get("/work/new/sub/b.m").get(3).condition, "iteration == 3");
assert.equal(shared.breakpoints.get("/work/new/renamed.m").get(4).enabled, false);
assert(shared.breakpoints.has("/work/older/c.m"));
trashOpenPaths("/work/new");
assert.deepEqual([...shared.breakpoints.keys()], ["/work/older/c.m"]);
assert.equal(debugPanelRenders, 3);
assert.equal(debugEditorRefreshes, 3);

// Finding 14: prefixes and the history cursor follow directory operations.
shared.folderHistory = ["/work", "/work/old", "/work/old/sub", "/work/older", "/work"];
shared.folderIndex = 2;
relocateFolderHistory("/work/old", "/work/new");
assert.deepEqual(shared.folderHistory, ["/work", "/work/new", "/work/new/sub", "/work/older", "/work"]);
assert.equal(shared.folderIndex, 2);
relocateFolderHistory("/work/new", null);
assert.deepEqual(shared.folderHistory, ["/work", "/work/older", "/work"]);
assert.equal(shared.folderIndex, 0);
shared.folderHistory = ["/work/old/sub", "/work", "/elsewhere"];
shared.folderIndex = 2;
relocateFolderHistory("/work/old", null);
assert.deepEqual(shared.folderHistory, ["/work", "/elsewhere"]);
assert.equal(shared.folderIndex, 1);

// Dirty buffers survive trash while stale folder-history entries are removed.
shared.currentFolder = "/work";
shared.tabs = [{ path: "/work/old/a.m", hash: "A", content: "dirty", dirty: true }];
shared.active = shared.tabs[0];
shared.folderHistory = ["/work/old", "/work"];
shared.folderIndex = 1;
trashOpenPaths("/work/old");
assert.equal(shared.tabs[0].content, "dirty");
assert.equal(shared.tabs[0].hash, null);
assert.deepEqual(shared.folderHistory, ["/work"]);
assert.equal(shared.folderIndex, 0);
console.log("FILE OPS UNIT PASS: stale hash retention, expected hashes, history rename/move/trash, dirty buffer preservation.");
