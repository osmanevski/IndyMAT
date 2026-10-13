"use strict";
const assert = require("node:assert/strict"), fs = require("node:fs");
const geometry = require("../frontend/figure_geometry_utils.cjs"), camera = require("../frontend/figure_camera_utils.cjs");
const { renderer, mock, canvas, timers } = require("./figure_peeling_harness.cjs");
const { FigureWebGL } = renderer(), timer = timers();
const axes = JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/surf_vector.json")).axes[0];
const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [.2, .2, .2], grid: [.3, .3, .3] };
try {
  const scene = geometry.buildScene(axes); scene.hasTransparency = true;
  const gl = mock(), node = canvas(gl), r = new FigureWebGL(node, [scene], () => assert.fail());
  const frame = { scene, camera: camera.createCamera(axes), viewport: { x: 0, y: 0, width: 400, height: 300 } };
  gl.calls.length = 0;
  assert.equal(r.readPixels(10, 20, 2), null, "never read an uncertified initial image");
  assert.equal(gl.calls.length, 0);
  r.render([frame], palette, 2);
  gl.calls.length = 0; assert.equal(r.readPixels(10, 20, 2), null); assert.equal(gl.calls.length, 0);
  timer.tick(); assert(!r.pending);
  // The mock default drawing buffer is all zero, as after compositor discard.
  // Read and draw framebuffer state are intentionally distinct here.
  const previous = gl.createFramebuffer(); gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous);
  gl.calls.length = 0;
  assert.deepEqual(r.readPixels(10, 20, 2), [10, 20, 30, 255]);
  const read = gl.calls.find(c => c.method === "readPixels");
  assert.equal(read.readFramebuffer, r.peeling.output.fbo);
  assert.equal(read.framebuffer, null, "pixel sampling must not change the draw framebuffer");
  assert.deepEqual(read.args.slice(0, 6), [20, 559, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE]);
  assert.deepEqual(gl.calls.filter(c => c.method === "bindFramebuffer").map(c => c.args),
    [[gl.READ_FRAMEBUFFER, r.peeling.output.fbo], [gl.READ_FRAMEBUFFER, previous]]);
  assert.equal(gl.getParameter(gl.READ_FRAMEBUFFER_BINDING), previous);
  gl.state.readError = true;
  assert.throws(() => r.readPixels(10, 20, 2), /read failure/);
  assert.equal(gl.getParameter(gl.READ_FRAMEBUFFER_BINDING), previous, "restore binding on a failed read");
  gl.state.readError = false;
  r.render([frame], palette, 2); gl.calls.length = 0;
  assert.equal(r.readPixels(10, 20, 2), null, "new pending frame cannot expose the old offscreen contents");
  assert.equal(gl.calls.length, 0);
  gl.deleteFramebuffer(previous); r.dispose(); assert.equal(gl.live.size, 0); assert.equal(timer.tasks.size, 0);

  scene.hasTransparency = false;
  const opaqueGL = mock(), opaque = new FigureWebGL(canvas(opaqueGL), [scene], () => assert.fail());
  opaqueGL.calls.length = 0; opaque.readPixels(10, 20, 2);
  assert.deepEqual(opaqueGL.calls.map(c => c.method), ["readPixels"], "opaque reads must gain no GL calls");
  opaque.dispose(); assert.equal(opaqueGL.live.size, 0);
} finally { timer.restore(); }
console.log("PASS transparent pixel reads use certified output FBO and restore read-only binding; initial/pending reads return null; opaque reads gain no GL calls.");
