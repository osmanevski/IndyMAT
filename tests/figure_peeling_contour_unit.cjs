"use strict";
const assert = require("node:assert/strict"), fs = require("node:fs");
const geometry = require("../frontend/figure_geometry_utils.cjs"), camera = require("../frontend/figure_camera_utils.cjs");
const { renderer, mock, canvas, timers } = require("./figure_peeling_harness.cjs");
const { FigureWebGL } = renderer(), timer = timers();
const axes = JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/surf_vector.json")).axes[0];
axes.visible = false;
const palette = { background: [0, 0, 0], text: [1, 1, 1], axis: [.2, .2, .2], grid: [.3, .3, .3] };
try {
  for (const transparent of [false, true]) {
    const scene = geometry.buildScene(axes), mesh = scene.meshes[0], edge = scene.edges[0];
    mesh.series = 1; mesh.source = { ...mesh.source, series: { ...mesh.source.series, role: "contourf" } };
    const makeEdge = series => ({ ...edge, series, positions: edge.positions.slice(0, 6), colors: edge.colors.slice(0, 6),
      alphas: transparent ? new Float32Array([.25, .75]) : undefined,
      source: { series: { kind: "patch3d", role: "contourf", no_area: true },
        vertexIndices: new Uint32Array([0, 1]), faceIndices: new Uint32Array([0, 0]) } });
    scene.edges = [makeEdge(0), makeEdge(2)]; scene.hasTransparency = transparent;
    const gl = mock(), node = canvas(gl), r = new FigureWebGL(node, [scene], () => assert.fail("fallback"));
    r.render([{ scene, camera: camera.createCamera(axes), viewport: { x: 0, y: 0, width: 400, height: 300 } }], palette, 2);
    const program = transparent ? r.peeling.peel : r.program;
    const draws = gl.calls.filter(c => c.program === program && /^draw/.test(c.method));
    assert.deepEqual(draws.slice(0, 3).map(c => c.method), ["drawArrays", "drawElements", "drawArrays"],
      "no-area contour children must keep their original slot around the fill");
    if (transparent) {
      const alphaUploads = gl.calls.filter(c => c.method === "bufferData" && c.args[1].length === 6 &&
        [...c.args[1]].every(value => value >= .25 && value <= .75));
      assert(alphaUploads.length >= 2, "both no-area edge opacities must reach captured upload");
      assert.deepEqual([...alphaUploads[0].args[1]], [.25, .25, .75, .75, .25, .75]);
      timer.tick();
    }
    r.dispose(); assert.equal(gl.live.size, 0); assert.equal(timer.tasks.size, 0);
  }
} finally { timer.restore(); }
console.log("PASS no-area contour borders preserve before/after-fill slots in opaque and peeling paths; endpoint alphas expand to stroke triangles.");
