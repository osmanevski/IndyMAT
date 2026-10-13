"use strict";
// A command/resource model, not a GLSL compiler or a graphics implementation.
const assert = require("node:assert/strict");
const path = require("node:path");
const Module = require("node:module");
const esbuild = require("esbuild");
function renderer() {
  const filename = path.resolve("frontend/figure_webgl.js"), loaded = new Module(filename, module);
  loaded.filename = filename;
  loaded.paths = Module._nodeModulePaths(path.dirname(filename));
  loaded._compile(esbuild.buildSync({ entryPoints: [filename], bundle: true, write: false,
    platform: "node", format: "cjs" }).outputFiles[0].text, filename);
  return loaded.exports;
}
function mock(options = {}) {
  let serial = 0, program, framebuffer = null, readFramebuffer = null, vao, query;
  const live = new Map(), calls = [], shaders = new Map(), uniforms = new Map(), links = new Map();
  const state = { available: true, overflow: false, lost: false, error: "NO_ERROR", ...options };
  const methods = {
    live, calls, shaders, links, state,
    getShaderParameter: () => state.compile !== false,
    getProgramParameter: () => state.link !== false,
    getUniformLocation: (p, name) => ({ p, name }),
    shaderSource: (shader, source) => shaders.set(shader, source),
    attachShader: (p, shader) => { if (!links.has(p)) links.set(p, []); links.get(p).push(shader); },
    getParameter: name => {
      calls.push({ method: "getParameter", args: [name] });
      return name === "READ_FRAMEBUFFER_BINDING" ? readFramebuffer : name === "DRAW_FRAMEBUFFER_BINDING" ? framebuffer : 4096;
    },
    checkFramebufferStatus: () => state.incomplete ? "INCOMPLETE" : "FRAMEBUFFER_COMPLETE",
    getQueryParameter: (q, property) => {
      assert(live.has(q), "query after deletion");
      return property === "QUERY_RESULT_AVAILABLE" ? state.available : state.overflow;
    },
    getError: () => state.error,
    isContextLost: () => state.lost,
    getExtension: () => ({ loseContext() { state.lost = true; } }),
    useProgram: p => { program = p; },
    bindFramebuffer: (target, f) => {
      calls.push({ method: "bindFramebuffer", args: [target, f] });
      if (target !== "READ_FRAMEBUFFER") framebuffer = f;
      if (target !== "DRAW_FRAMEBUFFER") readFramebuffer = f;
    },
    readPixels: (...args) => {
      calls.push({ method: "readPixels", args, framebuffer, readFramebuffer });
      if (state.readError) throw new Error("read failure");
      args[6].set(readFramebuffer ? [10, 20, 30, 255] : [0, 0, 0, 0]);
    },
    bindVertexArray: v => { vao = v; },
    beginQuery: (_, q) => { assert.equal(query, undefined); query = q; },
    endQuery: () => { assert(query); query = undefined; },
    drawArrays: (...args) => draw("drawArrays", args),
    drawElements: (...args) => draw("drawElements", args)
  };
  function draw(method, args) {
    const values = Object.fromEntries([...(uniforms.get(program) || [])]);
    calls.push({ method, args, program, framebuffer, vao, query, uniforms: values });
  }
  const gl = new Proxy(methods, { get(target, name) {
    if (name in target) return target[name];
    if (/^[A-Z_0-9]+$/.test(name)) return name;
    if (name.startsWith("create")) return (...args) => {
      if (name === "createShader") assert(["VERTEX_SHADER", "FRAGMENT_SHADER"].includes(args[0]), "shader type required");
      if (state.failAllocation === name) return null;
      const value = { kind: name.slice(6), id: ++serial, args };
      live.set(value, value.kind); calls.push({ method: name, args, value }); return value;
    };
    if (name.startsWith("delete")) return value => {
      assert(live.delete(value), "double deletion: " + name); calls.push({ method: name, value });
    };
    if (name.startsWith("uniform")) return (location, ...args) => {
      if (!uniforms.has(location.p)) uniforms.set(location.p, new Map());
      uniforms.get(location.p).set(location.name, args.length === 1 ? args[0] : args);
    };
    return (...args) => calls.push({ method: name, args });
  } });
  return gl;
}
function canvas(gl) {
  const listeners = new Map(), events = [];
  return { width: 800, height: 600, listeners, events, getContext: () => gl,
    addEventListener: (name, fn) => listeners.set(name, fn), removeEventListener: name => listeners.delete(name),
    dispatchEvent: event => { events.push(event.type); listeners.get(event.type)?.(event); } };
}
function timers() {
  const original = { setTimeout, clearTimeout }, tasks = new Map(); let serial = 0;
  global.setTimeout = fn => { tasks.set(++serial, fn); return serial; };
  global.clearTimeout = id => tasks.delete(id);
  return { tasks,
    tick() { const pending = [...tasks.values()]; tasks.clear(); pending.forEach(fn => fn()); },
    restore() { Object.assign(global, original); }
  };
}
function actualSources() {
  const gl = mock(), r = new (renderer().FigureWebGL)(canvas(gl), [], () => assert.fail());
  const pair = [...gl.links.values()][0].map(s => gl.shaders.get(s));
  r.dispose();
  return { vertex: pair[0], fragment: pair[1] };
}
module.exports = { renderer, mock, canvas, timers, actualSources };
