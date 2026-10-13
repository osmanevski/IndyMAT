"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { Peeling, shaderSources, LIMITS } = require("../frontend/figure_peeling.cjs");
const { mock, canvas, timers, actualSources, renderer } = require("./figure_peeling_harness.cjs");
const opaque = actualSources(), sources = shaderSources(opaque.vertex, opaque.fragment);
const timer = timers();
const groups = [{ viewport: [0, 0, 100, 100], draws: [
  { vao: "axis", count: 6, decoration: true, primitiveBase: 1, uniforms: new Map() },
  { vao: "mesh", count: 6, indexed: true, primitiveBase: 3, uniforms: new Map() }
] }];
let count = 0;
function test(name, fn) { fn(); assert.equal(timer.tasks.size, 0, "leaked task"); count++; console.log("PASS " + name); }
function setup(options) {
  const gl = mock(options), events = [], state = { now: 0 };
  const p = new Peeling(gl, sources, { pending: () => events.push(["pending"]),
    ready: data => events.push(["ready", data]), fail: (code, args) => events.push([code, args]) }, () => state.now);
  const render = (width = 100, height = 100) => p.render(groups, width, height, [1, 1, 1]);
  return { p, gl, events, state, render };
}
try {
  test("all shader stages use explicit types; partial constructor failures release resources", () => {
    for (const option of [{ compile: false }, { link: false }, { failAllocation: "createShader" }]) {
      const gl = mock(option);
      assert.throws(() => new Peeling(gl, sources, {})); assert.equal(gl.live.size, 0);
    }
  });
  test("queries hide output until available; axes background precedes resolve outside all nine selections", () => {
    const { p, gl, events, render } = setup({ available: false }); render();
    assert.deepEqual(events, [["pending"]]);
    assert.equal(gl.calls.filter(c => c.method === "drawElements" && c.program === p.peel).length, 9);
    const blends = gl.calls.filter(c => c.method === "blendFuncSeparate");
    assert.equal(blends.length, 9);
    assert.deepEqual(blends[8].args, [gl.ONE, gl.SRC_ALPHA, gl.ZERO, gl.ONE]);
    for (const call of blends.slice(0, 8)) {
      assert.deepEqual(call.args, [gl.DST_ALPHA, gl.ONE, gl.ZERO, gl.ONE_MINUS_SRC_ALPHA]);
    }
    assert(!gl.calls.some(c => c.vao === "axis" && c.program === p.peel));
    const overlay = gl.calls.findIndex(c => c.vao === "axis" && c.program === p.overlay);
    const resolve = gl.calls.findIndex(c => c.program === p.full && c.uniforms?.resolve === 1);
    assert(overlay >= 0 && overlay < resolve);
    assert(!gl.calls.some(c => c.method === "drawArrays" && c.framebuffer === null));
    timer.tick(); assert.equal(events.length, 1); assert.equal(timer.tasks.size, 1);
    gl.state.available = true; timer.tick();
    assert.equal(events[1][0], "ready"); assert.equal(events[1][1].frames, 1);
    assert(gl.calls.some(c => c.method === "drawArrays" && c.framebuffer === null));
    p.dispose(); p.dispose(); assert.equal(gl.live.size, 0);
  });
  test("K+1 overflow prevents presentation and reports exact lower bound", () => {
    const { p, gl, events, render } = setup({ overflow: true }); render(); timer.tick();
    assert.deepEqual(events[1], ["transparency_layers", { actual: 9, limit: 8 }]);
    assert(!gl.calls.some(c => c.method === "drawArrays" && c.framebuffer === null));
    assert(p.disposed); assert.equal(gl.live.size, 0);
  });
  test("blank deadline includes stalled queries without mistaking warm-up for steady cost", () => {
    const { p, gl, state, events, render } = setup({ available: false }); render();
    state.now = 251; timer.tick(); assert.equal(events.length, 1);
    state.now = LIMITS.blankMs + 1; timer.tick();
    assert.deepEqual(events[1], ["transparency_time", { actual_ms: 5001, limit_ms: 5000, phase: "blank_timeout" }]);
    assert(!gl.calls.some(c => c.method === "finish")); assert(p.disposed); assert.equal(gl.live.size, 0);
  });
  test("warm-up then three consecutive slow completed frames; resize rearms warm-up", () => {
    const { p, state, events, gl, render } = setup();
    for (let i = 0; i < 2; i++) { render(); state.now += 800; timer.tick(); assert.equal(events.at(-1)[1].warmup, true); }
    render(); state.now += 300; timer.tick(); assert.equal(events.at(-1)[1].slowFrames, 1);
    const oldTargets = p.targetsResources.map(([, resource]) => resource);
    render(200, 100); assert(oldTargets.every(resource => !gl.live.has(resource)));
    state.now += 300; timer.tick(); assert.equal(events.at(-1)[1].warmup, true);
    for (let i = 0; i < 3; i++) { render(200, 100); state.now += 300; timer.tick(); assert.equal(events.at(-1)[0], "ready"); }
    render(200, 100); state.now += 300; timer.tick(); assert.equal(events.at(-1)[0], "transparency_time"); assert.equal(gl.live.size, 0);
  });
  test("replacement cancels stale query callback; resize and dispose free every allocation", () => {
    const { p, events, gl, render } = setup({ available: false }); render();
    const stale = [...timer.tasks.values()][0], oldQuery = p.queries[0][1];
    render(120, 120); assert(!gl.live.has(oldQuery)); stale(); assert.equal(events.length, 2);
    gl.state.available = true; timer.tick(); assert.equal(events.at(-1)[0], "ready");
    p.dispose(); stale(); assert.equal(gl.live.size, 0);
  });
  test("framebuffer, allocation, dimension and GL errors fail closed", () => {
    for (const option of [{ incomplete: true }, { failAllocation: "createTexture" }, { error: "INVALID_OPERATION" }]) {
      const { gl, events, render } = setup(option); render();
      assert.equal(events.at(-1)[0], "transparency_unavailable"); assert.equal(gl.live.size, 0);
    }
    const { gl, events, render } = setup(); render(5000, 100);
    assert.equal(events.at(-1)[0], "transparency_unavailable"); assert.equal(gl.live.size, 0);
  });
  test("context loss and asynchronous query failure cannot reveal pending output", () => {
    for (const throwQuery of [false, true]) {
      const { gl, events, render } = setup(); render();
      if (throwQuery) gl.getQueryParameter = () => { throw new Error("query failure"); };
      else gl.state.lost = true;
      timer.tick(); assert.equal(events.at(-1)[0], "transparency_unavailable"); assert.equal(gl.live.size, 0);
    }
  });
  test("renderer events, pending picking, context-loss cancellation and opaque bypass", () => {
    const geometry = require("../frontend/figure_geometry_utils.cjs"), camera = require("../frontend/figure_camera_utils.cjs");
    const axes = JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/surf_vector.json")).axes[0];
    const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [.2, .2, .2], grid: [.3, .3, .3] };
    const { FigureWebGL } = renderer();
    const scene = geometry.buildScene(axes); scene.hasTransparency = true;
    scene.meshes[0].alphas = new Float32Array(scene.meshes[0].positions.length / 3).fill(.5);
    const frame = { scene, camera: camera.createCamera(axes), viewport: { x: 0, y: 0, width: 400, height: 300 } };
    const gl = mock(), node = canvas(gl), failures = [];
    const r = new FigureWebGL(node, [scene], code => failures.push(code));
    r.render([frame], palette, 2); assert(r.pending); assert.equal(r.readPixels(0, 0, 1), null);
    assert.deepEqual(node.events, ["figuretransparencypending"]); timer.tick();
    assert(!r.pending); assert.equal(node.events.at(-1), "figuretransparencyready");
    r.render([frame], palette, 2);
    node.listeners.get("webglcontextlost")({ preventDefault() {} }); timer.tick();
    assert.deepEqual(failures, ["transparency_unavailable"]); assert.equal(gl.live.size, 0);
    delete scene.hasTransparency; delete scene.meshes[0].alphas;
    const opaqueGL = mock(), o = new FigureWebGL(canvas(opaqueGL), [scene], () => assert.fail());
    o.render([frame], palette, 2); assert.equal(o.peeling, null);
    assert(!opaqueGL.calls.some(c => ["createQuery", "createFramebuffer", "createTexture"].includes(c.method)));
    o.dispose(); assert.equal(opaqueGL.live.size, 0);
  });
} finally { timer.restore(); }
assert.deepEqual(LIMITS, { layers: 8, warmupFrames: 2, slowFrames: 3, timeMs: 250, blankMs: 5000 });
console.log(`PASS ${count} peeling command/lifecycle cases; mocked GL, no GPU timing or shader compilation claim.`);
