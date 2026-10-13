"use strict";

// WebGL2 core formats only. Depth is deliberately quantized to 24 bits;
// equal-depth primitives are submitted in ascending ID order and LEQUAL picks
// the last one. Peeling then visits decreasing IDs at that depth.
// The eight-layer bound includes an opaque terminating fragment; transparent
// fragments behind it, alpha-zero fragments, background and axes do not count.
const { TIMING_LIMITS, TransparencyTiming } = require("./figure_transparency_timing.cjs");
const LIMITS = Object.freeze({ layers: 8, ...TIMING_LIMITS });
function shaderSources(vertex, fragment) {
  return {
    vertex: vertex.replace("precision highp float;", `precision highp float;
precision highp int;
layout(location=4) in float alpha;
uniform highp uint primitiveBase;
smooth out float opacity;
flat out highp uint primitiveID;`).replace("  world =", `  opacity = alpha;
  primitiveID = primitiveBase + uint(gl_VertexID / 3);
  world =`),
    fragment: fragment.replace("precision highp float;", `precision highp float;
precision highp int;
smooth in float opacity;
flat in highp uint primitiveID;
uniform highp uint primitiveBase;
uniform highp usampler2D previousKey;
uniform bool firstLayer;
layout(location=1) out highp uvec2 outputKey;`).replace("out vec4 outputColor;", "layout(location=0) out vec4 outputColor;").replace("  outputColor = vec4(rgb,1.0);", `  if (opacity <= 0.0 || primitiveID < primitiveBase) discard;
  uint depth = min(uint(floor(clamp(gl_FragCoord.z,0.0,1.0)*16777215.0+0.5)),16777215u);
  if (!firstLayer) {
    ivec2 pixel = ivec2(gl_FragCoord.xy);
    uvec2 previous = texelFetch(previousKey,pixel,0).rg;
    if (previous.x == 0xffffffffu || (previous.y & 0x80000000u) != 0u || depth < previous.x || (depth == previous.x && primitiveID >= (previous.y & 0x7fffffffu))) discard;
  }
  gl_FragDepth = float(depth)/16777215.0;
  outputKey = uvec2(depth,primitiveID | (opacity >= 1.0 ? 0x80000000u : 0u));
  outputColor = vec4(rgb,opacity);`),
    fullVertex: FULL_VERTEX,
    fullFragment: FULL_FRAGMENT,
    overlayVertex: vertex,
    overlayFragment: fragment
  };
}
const FULL_VERTEX = `#version 300 es
precision highp float;
void main() {
  vec2 p = gl_VertexID == 0 ? vec2(-1,-1) : gl_VertexID == 1 ? vec2(3,-1) : vec2(-1,3);
  gl_Position = vec4(p,0,1);
}`;
const FULL_FRAGMENT = `#version 300 es
precision highp float;
uniform highp sampler2D image;
uniform bool resolve;
out vec4 outputColor;
void main() {
  vec4 p = texelFetch(image,ivec2(gl_FragCoord.xy),0);
  // Accumulation stores premultiplied RGB and remaining transmittance.
  outputColor = resolve ? p : vec4(p.rgb*p.a,p.a);
}`;

// Deindex only the transparent figure's meshes so gl_VertexID / 3 is a stable
// triangle ID even where the original indexed mesh shares vertices.
function expandMesh(mesh) {
  if (!mesh.indices) return mesh;
  const positions = new Float32Array(mesh.indices.length * 3);
  const colors = new Float32Array(positions.length);
  const alphas = mesh.alphas ? new Float32Array(mesh.indices.length) : undefined;
  mesh.indices.forEach((index, i) => {
    positions.set(mesh.positions.subarray(index * 3, index * 3 + 3), i * 3);
    colors.set(mesh.colors.subarray(index * 3, index * 3 + 3), i * 3);
    if (alphas) alphas[i] = mesh.alphas[index];
  });
  return { positions, colors, alphas, indices: Uint32Array.from({ length: mesh.indices.length }, (_, i) => i) };
}

// Capture the existing geometry preparation once. This deliberately records a
// narrow draw contract, not arbitrary GL state: clip/marker uniforms, immutable
// dynamic attributes and canonical triangle order. No GL calls occur here.
function capture(gl, uniforms, dynamic, draw) {
  const names = new Map(Object.entries(uniforms).map(([name, location]) => [location, name]));
  const state = new Map(), values = new Map(), enabled = new Set([0, 1, 2]);
  const groups = [];
  let group, vao, buffer, depth = true, primitiveBase = 1;
  const methods = {
    viewport: (...viewport) => { group = { viewport, draws: [] }; groups.push(group); },
    bindVertexArray: value => { vao = value; },
    bindBuffer: (_, value) => { buffer = value; },
    bufferData: (_, data) => values.set(buffer, data.slice()),
    enableVertexAttribArray: index => enabled.add(index),
    disableVertexAttribArray: index => enabled.delete(index),
    enable: cap => { if (cap === gl.DEPTH_TEST) depth = true; },
    disable: cap => { if (cap === gl.DEPTH_TEST) depth = false; }
  };
  const record = (indexed, count) => {
    if (!count) return;
    if (!group || count % 3 || primitiveBase + count / 3 >= 0x80000000) throw new Error("capture triangles");
    const item = { vao, indexed, count, uniforms: new Map(state), decoration: !depth, primitiveBase };
    if (vao === dynamic.vao) item.attributes = dynamic.attributes.map((attribute, i) => enabled.has(i) ? values.get(attribute) : null);
    group.draws.push(item);
    primitiveBase += count / 3;
  };
  methods.drawElements = (_, count) => record(true, count);
  methods.drawArrays = (_, start, count) => { if (start !== 0) throw new Error("capture offset"); record(false, count); };
  const proxy = new Proxy(gl, { get(target, name) {
    if (methods[name]) return methods[name];
    if (typeof name === "string" && name.startsWith("uniform")) return (location, ...args) => {
      const key = names.get(location);
      if (!key) throw new Error("capture uniform");
      state.set(key, { method: name, args: args.map(value => ArrayBuffer.isView(value) || Array.isArray(value) ? value.slice() : value) });
    };
    if (typeof target[name] === "function") return () => {};
    return target[name];
  } });
  draw(proxy);
  return groups;
}

class Peeling {
  constructor(gl, sources, callbacks, clock = () => performance.now()) {
    this.gl = gl;
    this.callbacks = callbacks;
    this.clock = clock;
    this.timing = new TransparencyTiming(clock);
    this.blankStarted = null;
    this.resources = [];
    this.targetsResources = [];
    this.geometryResources = [];
    this.queries = [];
    this.generation = 0;
    this.frames = 0;
    this.width = 0;
    this.height = 0;
    this.disposed = false;
    this.timer = null;
    try {
      this.peel = this.program(sources.vertex, sources.fragment);
      this.full = this.program(sources.fullVertex, sources.fullFragment);
      this.overlay = this.program(sources.overlayVertex, sources.overlayFragment);
      this.fullVAO = this.allocate("VertexArray");
      this.locations = new Map();
    } catch (error) { this.dispose(); throw error; }
  }
  allocate(kind, bucket = this.resources, ...args) {
    const value = this.gl["create" + kind](...args);
    if (!value) throw new Error("allocation " + kind);
    bucket.push([kind, value]);
    return value;
  }
  release(bucket) {
    for (const [kind, value] of bucket.splice(0)) this.gl["delete" + kind](value);
  }
  program(vertex, fragment) {
    const gl = this.gl, program = this.allocate("Program");
    for (const [type, source] of [[gl.VERTEX_SHADER, vertex], [gl.FRAGMENT_SHADER, fragment]]) {
      const shader = this.allocate("Shader", this.resources, type);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error("peeling compile");
      gl.attachShader(program, shader);
    }
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error("peeling link");
    return program;
  }
  uniform(program, name) {
    if (!this.locations.has(program)) this.locations.set(program, new Map());
    const locations = this.locations.get(program);
    if (!locations.has(name)) locations.set(name, this.gl.getUniformLocation(program, name));
    return locations.get(name);
  }
  texture(format) {
    const gl = this.gl, texture = this.allocate("Texture", this.targetsResources);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texStorage2D(gl.TEXTURE_2D, 1, format, this.width, this.height);
    return texture;
  }
  target(peel) {
    const gl = this.gl, fbo = this.allocate("Framebuffer", this.targetsResources);
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    const color = this.texture(gl.RGBA8);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, color, 0);
    let key;
    if (peel) {
      key = this.texture(gl.RG32UI);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT1, gl.TEXTURE_2D, key, 0);
      const depth = this.texture(gl.DEPTH_COMPONENT24);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.TEXTURE_2D, depth, 0);
    }
    gl.drawBuffers(peel ? [gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1] : [gl.COLOR_ATTACHMENT0]);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error("peeling framebuffer");
    return { fbo, color, key };
  }
  resize(width, height) {
    if (this.width === width && this.height === height) return;
    this.release(this.targetsResources);
    this.width = width;
    this.height = height;
    this.frames = 0;
    this.timing.reset();
    if (!(width > 0 && height > 0) || width > this.gl.getParameter(this.gl.MAX_TEXTURE_SIZE) || height > this.gl.getParameter(this.gl.MAX_TEXTURE_SIZE)) throw new Error("peeling dimensions");
    this.targets = [this.target(true), this.target(true)];
    this.accumulation = this.target(false);
    // Keep the completed figure offscreen until every axes overflow query
    // resolves. The default drawing buffer is not preserved across tasks.
    this.output = this.target(false);
  }
  upload(groups) {
    const gl = this.gl;
    this.release(this.geometryResources);
    for (const group of groups) for (const draw of group.draws) if (draw.attributes) {
      draw.vao = this.allocate("VertexArray", this.geometryResources);
      gl.bindVertexArray(draw.vao);
      draw.attributes.forEach((values, index) => {
        if (!values) return;
        const buffer = this.allocate("Buffer", this.geometryResources);
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        gl.bufferData(gl.ARRAY_BUFFER, values, gl.STATIC_DRAW);
        gl.enableVertexAttribArray(index);
        gl.vertexAttribPointer(index, index === 4 ? 1 : index === 3 ? 4 : index === 2 ? 2 : 3, gl.FLOAT, false, 0, 0);
      });
    }
  }
  cancelPending() {
    this.generation++;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.release(this.queries);
  }
  fail(code, args = {}) {
    this.dispose();
    this.callbacks.fail(code, args);
  }
  render(groups, width, height, background, started = this.clock()) {
    if (this.disposed) return;
    this.cancelPending();
    const generation = this.generation, gl = this.gl;
    if (this.blankStarted === null) this.blankStarted = started;
    this.callbacks.pending();
    try {
      this.resize(width, height);
      // One-time target allocation is excluded; recurring uploads are measured.
      const frameStarted = this.timing.begin();
      this.upload(groups);
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.output.fbo);
      gl.disable(gl.SCISSOR_TEST);
      gl.clearColor(...background, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.disable(gl.CULL_FACE);
      gl.disable(gl.POLYGON_OFFSET_FILL);
      gl.enable(gl.SCISSOR_TEST);
      const replay = (program, draw) => {
        for (const [name, { method, args }] of draw.uniforms) gl[method](this.uniform(program, name), ...args);
        gl.bindVertexArray(draw.vao);
        gl.vertexAttrib1f(4, 1);
        if (draw.indexed) gl.drawElements(gl.TRIANGLES, draw.count, gl.UNSIGNED_INT, 0);
        else gl.drawArrays(gl.TRIANGLES, 0, draw.count);
      };
      const select = (group, layer) => {
        const current = this.targets[layer % 2], previous = this.targets[(layer + 1) % 2];
        gl.bindFramebuffer(gl.FRAMEBUFFER, current.fbo);
        gl.disable(gl.BLEND);
        gl.enable(gl.DEPTH_TEST);
        gl.depthMask(true);
        gl.depthFunc(gl.LEQUAL);
        gl.clearBufferfv(gl.COLOR, 0, new Float32Array(4));
        gl.clearBufferuiv(gl.COLOR, 1, new Uint32Array([0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff]));
        gl.clearBufferfv(gl.DEPTH, 0, new Float32Array([1]));
        gl.useProgram(this.peel);
        gl.uniform1i(this.uniform(this.peel, "firstLayer"), layer === 0 ? 1 : 0);
        gl.uniform1i(this.uniform(this.peel, "previousKey"), 0);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, previous.key);
        for (const draw of group.draws) {
          if (draw.decoration) continue;
          gl.uniform1ui(this.uniform(this.peel, "primitiveBase"), draw.primitiveBase);
          replay(this.peel, draw);
        }
        return current;
      };
      const fullscreen = (texture, resolve, destination) => {
        gl.bindFramebuffer(gl.FRAMEBUFFER, destination);
        gl.disable(gl.DEPTH_TEST);
        gl.depthMask(false);
        gl.useProgram(this.full);
        gl.uniform1i(this.uniform(this.full, "image"), 0);
        gl.uniform1i(this.uniform(this.full, "resolve"), resolve ? 1 : 0);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, texture);
        // No sampled texture may also be an active render attachment.
        gl.activeTexture(gl.TEXTURE1);
        gl.bindTexture(gl.TEXTURE_2D, null);
        gl.bindVertexArray(this.fullVAO);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      };
      for (const group of groups) {
        gl.viewport(...group.viewport);
        gl.scissor(...group.viewport);
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.accumulation.fbo);
        gl.disable(gl.BLEND);
        gl.clearBufferfv(gl.COLOR, 0, new Float32Array([0, 0, 0, 1]));
        for (let layer = 0; layer < LIMITS.layers; layer++) {
          if (this.blankExpired()) return;
          const current = select(group, layer);
          gl.enable(gl.BLEND);
          gl.blendEquation(gl.FUNC_ADD);
          gl.blendFuncSeparate(gl.DST_ALPHA, gl.ONE, gl.ZERO, gl.ONE_MINUS_SRC_ALPHA);
          fullscreen(current.color, false, this.accumulation.fbo);
        }
        const query = this.allocate("Query", this.queries);
        gl.beginQuery(gl.ANY_SAMPLES_PASSED, query);
        select(group, LIMITS.layers);
        gl.endQuery(gl.ANY_SAMPLES_PASSED);
        // Match opaque rendering: decorate the background before the data.
        // Decoration never participates in depth peeling or its layer budget.
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.output.fbo);
        gl.disable(gl.BLEND);
        gl.disable(gl.DEPTH_TEST);
        gl.depthMask(false);
        gl.useProgram(this.overlay);
        for (const draw of group.draws) if (draw.decoration) replay(this.overlay, draw);
        gl.enable(gl.BLEND);
        gl.blendEquation(gl.FUNC_ADD);
        // C = accumulated premultiplied RGB + transmittance * background;
        // the decorated output framebuffer has opaque destination alpha.
        gl.blendFuncSeparate(gl.ONE, gl.SRC_ALPHA, gl.ZERO, gl.ONE);
        fullscreen(this.accumulation.color, true, this.output.fbo);
      }
      gl.depthMask(true);
      gl.disable(gl.SCISSOR_TEST);
      gl.bindVertexArray(null);
      gl.flush();
      if (gl.getError() !== gl.NO_ERROR) throw new Error("peeling GL error");
      const poll = () => {
        if (this.disposed || generation !== this.generation) return;
        try {
          if (gl.isContextLost()) throw new Error("peeling context lost");
          const elapsed = this.clock() - started;
          const ready = this.queries.every(([, query]) => gl.getQueryParameter(query, gl.QUERY_RESULT_AVAILABLE));
          if (this.blankExpired()) return;
          if (!ready) { this.timer = setTimeout(poll, 8); return; }
          const overflow = this.queries.some(([, query]) => gl.getQueryParameter(query, gl.QUERY_RESULT));
          this.cancelPending();
          if (overflow) { this.fail("transparency_layers", { actual: LIMITS.layers + 1, limit: LIMITS.layers }); return; }
          const timing = this.timing.complete(frameStarted);
          if (timing.failure) { this.fail("transparency_time", timing.failure); return; }
          gl.viewport(0, 0, width, height);
          gl.disable(gl.SCISSOR_TEST);
          gl.disable(gl.BLEND);
          fullscreen(this.output.color, false, null);
          gl.depthMask(true);
          gl.bindVertexArray(null);
          if (gl.getError() !== gl.NO_ERROR) throw new Error("peeling presentation error");
          this.frames++;
          this.blankStarted = null;
          this.callbacks.ready({ submissionToReadyMs: elapsed, ...timing.diagnostics, limit: LIMITS.layers, blankLimitMs: LIMITS.blankMs });
        } catch {
          this.fail("transparency_unavailable");
        }
      };
      // A query result cannot become available in the same event-loop task.
      this.timer = setTimeout(poll, 0);
    } catch (error) {
      this.fail(error.reason_code || "transparency_unavailable", error.reason_args || {});
    }
  }
  blankExpired() {
    const elapsed = this.clock() - this.blankStarted;
    if (elapsed <= LIMITS.blankMs) return false;
    this.fail("transparency_time", { actual_ms: elapsed, limit_ms: LIMITS.blankMs, phase: "blank_timeout" });
    return true;
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.cancelPending();
    this.release(this.geometryResources);
    this.release(this.targetsResources);
    this.release(this.resources);
  }
}
module.exports = { LIMITS, TransparencyTiming, shaderSources, expandMesh, capture, Peeling };
