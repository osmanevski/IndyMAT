import cameraMath from "./figure_camera_utils.cjs";
import geometry from "./figure_geometry_utils.cjs";
import colors from "./figure_color_utils.cjs";

// One program handles world triangles and R1's pixel-space triangles. All
// positions retain a world coordinate for clipping against the axes box.
const VERTEX = `#version 300 es
precision highp float;
layout(location=0) in vec3 position;
layout(location=1) in vec3 color;
layout(location=2) in vec2 uv;
uniform mat4 matrix;
uniform bool screen;
uniform vec2 viewport;
uniform vec3 origin, pixelX, pixelY, depthVector;
out vec3 world, ink;
out vec2 markerUV;
void main() {
  world = screen ? origin + position.x*pixelX + position.y*pixelY + position.z*depthVector : position;
  gl_Position = screen ? vec4(position.x*2.0/viewport.x-1.0, 1.0-position.y*2.0/viewport.y, position.z*2.0-1.0, 1.0) : matrix*vec4(position,1.0);
  ink = color;
  markerUV = uv;
}`;
const FRAGMENT = `#version 300 es
precision highp float;
in vec3 world, ink;
in vec2 markerUV;
uniform vec3 low, high, faceInk, edgeInk;
uniform bool clip;
uniform float slack;
uniform int marker;
uniform bool hasFace, hasEdge;
uniform float border;
out vec4 outputColor;
float shape(vec2 p) {
  if (marker == 2) return max(abs(p.x),abs(p.y));
  if (marker >= 3 && marker <= 6) {
    p.y = -p.y;
    if (marker == 4) p = -p;
    if (marker == 5) p = vec2(-p.y,p.x);
    if (marker == 6) p = vec2(p.y,-p.x);
    return max(-p.y, max(0.8660254*p.x+0.5*p.y,-0.8660254*p.x+0.5*p.y))*2.0;
  }
  return length(p);
}
void main() {
  if (clip && (any(lessThan(world,low-vec3(slack))) || any(greaterThan(world,high+vec3(slack))))) discard;
  vec3 rgb = ink;
  if (marker > 0) {
    vec2 p = markerUV;
    if (marker >= 8) {
      float crossLine = min(abs(p.x),abs(p.y));
      float diagonal = min(abs(p.x-p.y),abs(p.x+p.y))*0.7071068;
      float d = marker == 8 ? crossLine : marker == 9 ? diagonal : min(crossLine,diagonal);
      if (!hasEdge || d > border) discard;
      rgb = edgeInk;
    } else {
      float d = shape(p);
      if (d > 1.0) discard;
      if (hasEdge && (d > 1.0-border || marker == 7)) rgb = edgeInk;
      else if (hasFace) rgb = faceInk;
      else discard;
    }
  }
  outputColor = vec4(rgb,1.0);
}`;
const MARKERS = { o: 1, s: 2, square: 2, "^": 3, v: 4, ">": 5, "<": 6, ".": 7, "+": 8, x: 9, "*": 10 };
let activeRenderer = null;

// Bound expansion work to the visible viewport, even at maximum local zoom.
// Dash phase is measured along the original run, including clipped portions.
export function screenPolyline(projected, width, breaks = [0, projected.length], style = "-", viewport) {
  const pattern = { "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[style];
  const output = [];
  if (style === "none" || width <= 0) return new Float32Array();
  const cycle = pattern?.reduce((sum, value) => sum + value, 0);
  for (let run = 0; run < breaks.length - 1; run++) {
    let phase = 0;
    for (let i = breaks[run] + 1; i < breaks[run + 1]; i++) {
      const a = projected[i - 1];
      const b = projected[i];
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const length = Math.hypot(dx, dy);
      if (!Number.isFinite(length) || !length) continue;
      let low = 0;
      let high = 1;
      for (const [start, delta, min, max] of [[a.x, dx, -width, viewport.width + width], [a.y, dy, -width, viewport.height + width]]) {
        if (!delta) {
          if (start < min || start > max) high = -1;
        } else {
          const t0 = (min - start) / delta;
          const t1 = (max - start) / delta;
          low = Math.max(low, Math.min(t0, t1));
          high = Math.min(high, Math.max(t0, t1));
        }
      }
      const lerp = (t) => ({ x: a.x + dx * t, y: a.y + dy * t, depth: a.depth + (b.depth - a.depth) * t });
      const emit = (start, end) => {
        const expanded = geometry.expandPolyline([lerp(start), lerp(end)], width);
        for (const value of expanded) output.push(value);
      };
      if (low < high) {
        if (!pattern) emit(low, high);
        else {
          let offset = low * length;
          const end = high * length;
          while (offset < end) {
            let remaining = (phase + offset) % cycle;
            let part = 0;
            while (remaining >= pattern[part]) remaining -= pattern[part++];
            const step = Math.min(end - offset, pattern[part] - remaining);
            if (part % 2 === 0) emit(offset / length, (offset + step) / length);
            if (offset + step === offset) break;
            offset += step;
          }
        }
      }
      if (pattern) phase = (phase + length) % cycle;
    }
  }
  return new Float32Array(output);
}

export class FigureWebGL {
  constructor(canvas, scenes, failed) {
    activeRenderer?.dispose();
    this.canvas = canvas;
    this.failed = failed;
    this.buffers = [];
    this.vaos = [];
    this.shaders = [];
    this.program = null;
    this.frame = null;
    this.disposed = false;
    this.lost = (event) => {
      event.preventDefault();
      this.fail("context_lost");
    };
    canvas.addEventListener("webglcontextlost", this.lost);
    try {
      this.gl = canvas.getContext("webgl2", { alpha: false, antialias: true });
    } catch {
      this.gl = null;
    }
    if (!this.gl) {
      this.dispose();
      throw Object.assign(new Error("webgl_unavailable"), { reason_code: "webgl_unavailable" });
    }
    activeRenderer = this;
    try {
      this.program = this.gl.createProgram();
      if (!this.program) throw new Error("program");
      for (const [type, source] of [[this.gl.VERTEX_SHADER, VERTEX], [this.gl.FRAGMENT_SHADER, FRAGMENT]]) {
        const shader = this.gl.createShader(type);
        if (!shader) throw new Error("shader");
        this.shaders.push(shader);
        this.gl.shaderSource(shader, source);
        this.gl.compileShader(shader);
        if (!this.gl.getShaderParameter(shader, this.gl.COMPILE_STATUS)) throw new Error("compile");
        this.gl.attachShader(this.program, shader);
      }
      this.gl.linkProgram(this.program);
      if (!this.gl.getProgramParameter(this.program, this.gl.LINK_STATUS)) throw new Error("link");
      this.uniforms = Object.fromEntries(["matrix", "screen", "viewport", "origin", "pixelX", "pixelY", "depthVector", "low", "high", "marker", "faceInk", "edgeInk", "hasFace", "hasEdge", "border", "clip", "slack"].map((name) => [name, this.gl.getUniformLocation(this.program, name)]));
      this.meshes = scenes.map((scene) => scene.meshes.map((mesh) => this.upload(mesh)));
      this.dynamic = this.upload({ positions: new Float32Array(), colors: new Float32Array() });
    } catch {
      this.dispose();
      throw Object.assign(new Error("shader_failure"), { reason_code: "shader_failure" });
    }
  }
  buffer(target, values, usage) {
    const gl = this.gl;
    const buffer = gl.createBuffer();
    if (!buffer) throw new Error("buffer");
    this.buffers.push(buffer);
    gl.bindBuffer(target, buffer);
    gl.bufferData(target, values, usage);
    return buffer;
  }
  upload(mesh) {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    if (!vao) throw new Error("vertex array");
    this.vaos.push(vao);
    gl.bindVertexArray(vao);
    const attributes = [mesh.positions, mesh.colors, mesh.uv || new Float32Array(mesh.positions.length / 3 * 2)].map((values, index) => {
      const buffer = this.buffer(gl.ARRAY_BUFFER, values, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(index);
      gl.vertexAttribPointer(index, index === 2 ? 2 : 3, gl.FLOAT, false, 0, 0);
      return buffer;
    });
    const indices = mesh.indices ? this.buffer(gl.ELEMENT_ARRAY_BUFFER, mesh.indices, gl.STATIC_DRAW) : null;
    return { vao, attributes, indices, count: mesh.indices?.length || mesh.positions.length / 3 };
  }
  fail(code) {
    if (this.disposed) return;
    this.dispose();
    this.failed(code);
  }
  requestRender(draw) {
    if (this.disposed || this.frame !== null) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      if (!this.disposed) draw();
    });
  }
  // Pixel geometry reuses three buffers; no resource accumulation per gesture.
  pixels(positions, rgb, uv) {
    if (!positions.length) return;
    const gl = this.gl;
    const values = [positions, rgb, uv || new Float32Array(positions.length / 3 * 2)];
    gl.bindVertexArray(this.dynamic.vao);
    values.forEach((value, index) => {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.dynamic.attributes[index]);
      gl.bufferData(gl.ARRAY_BUFFER, value, gl.DYNAMIC_DRAW);
    });
    gl.drawArrays(gl.TRIANGLES, 0, positions.length / 3);
  }
  solid(positions, rgb) {
    const values = new Float32Array(positions.length);
    for (let i = 0; i < values.length; i += 3) values.set(rgb, i);
    this.pixels(positions, values);
  }
  render(frames, palette, dpr) {
    if (this.disposed) return;
    const gl = this.gl;
    const u = this.uniforms;
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(...palette.background, 1);
    gl.clearDepth(1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.disable(gl.CULL_FACE);
    gl.disable(gl.BLEND);
    gl.enable(gl.SCISSOR_TEST);
    gl.useProgram(this.program);
    frames.forEach(({ scene, camera, viewport: vp }, axis) => {
      const x = Math.round(vp.x * dpr);
      const y = Math.round((this.canvas.height / dpr - vp.y - vp.height) * dpr);
      const w = Math.round(vp.width * dpr);
      const h = Math.round(vp.height * dpr);
      gl.viewport(x, y, w, h);
      gl.scissor(x, y, w, h);
      gl.clear(gl.DEPTH_BUFFER_BIT);
      gl.uniformMatrix4fv(u.matrix, false, cameraMath.matrices(camera, vp.width, vp.height).viewProjection);
      gl.uniform3fv(u.low, scene.bounds.min);
      gl.uniform3fv(u.high, scene.bounds.max);
      gl.uniform1i(u.marker, 0);
      gl.uniform1i(u.screen, 1);
      gl.uniform2f(u.viewport, vp.width, vp.height);
      const local = { width: vp.width, height: vp.height };
      const ray = cameraMath.unprojectRay(camera, local, 0, 0);
      const rayX = cameraMath.unprojectRay(camera, local, 1, 0);
      const rayY = cameraMath.unprojectRay(camera, local, 0, 1);
      gl.uniform3fv(u.origin, ray.origin);
      const pixelX = rayX.origin.map((v, i) => v - ray.origin[i]);
      gl.uniform3fv(u.pixelX, pixelX);
      gl.uniform3fv(u.pixelY, rayY.origin.map((v, i) => v - ray.origin[i]));
      gl.uniform3fv(u.depthVector, ray.direction.map((v) => v * (camera.far - camera.near)));
      const project = (positions) => Array.from({ length: positions.length / 3 }, (_, i) => cameraMath.project(camera, local, positions.subarray(i * 3, i * 3 + 3)));
      const adjust = (rgb) => colors.adjustForContrast(rgb, palette.background, palette.text);
      // A pixel-wide stroke lying on a limit plane leaves the box by half its
      // width; allow exactly that much so boundary data is not cut in half.
      const unit = Math.hypot(...pixelX);
      const slack = (pixels) => gl.uniform1f(u.slack, 0.0001 + unit * pixels / 2);
      if (scene.axes) {
        // Back walls: grid and outline are decoration in the theme's own
        // grid/axis tokens (like the 2D view). All data is clipped to the box,
        // so the walls are behind everything: they are drawn first, unclipped
        // and without depth, and a marker on a wall is never pierced by them.
        const box = cameraMath.axisBox(camera, scene.axes);
        const keys = ["x", "y", "z"];
        const segment = (start, end, output) => {
          const expanded = screenPolyline(project(new Float32Array([...start, ...end])), 1, undefined, "-", local);
          for (let i = 0; i < expanded.length; i += 3) output.push(expanded[i], expanded[i + 1], expanded[i + 2]);
        };
        const grid = [];
        const axisLines = [];
        for (const face of box.faces.filter((item) => item.away)) {
          const fixed = keys.indexOf(face.axis);
          for (let axis = 0; axis < 3; axis++) {
            if (axis === fixed || !scene.axes.grid[keys[axis]]) continue;
            const along = [0, 1, 2].find((i) => i !== axis && i !== fixed);
            for (const tick of box.axes[keys[axis]].ticks) {
              const start = [0, 0, 0];
              start[fixed] = face.sign * box.extent[fixed];
              start[axis] = tick.position[axis];
              start[along] = -box.extent[along];
              const end = start.slice();
              end[along] = box.extent[along];
              segment(start, end, grid);
            }
          }
        }
        const chosen = keys.map((key) => box.axes[key].edge);
        for (const edge of box.edges) {
          if (chosen.includes(edge)) segment(edge.start, edge.end, axisLines);
          else if (edge.behind) segment(edge.start, edge.end, grid);
        }
        gl.uniform1i(u.clip, 0);
        gl.disable(gl.DEPTH_TEST);
        this.solid(new Float32Array(grid), palette.grid);
        this.solid(new Float32Array(axisLines), palette.axis);
        gl.enable(gl.DEPTH_TEST);
      }
      gl.uniform1i(u.screen, 0);
      gl.uniform1i(u.clip, 1);
      gl.uniform1f(u.slack, 0.00001);
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(1, 1);
      for (const mesh of this.meshes[axis]) {
        gl.bindVertexArray(mesh.vao);
        gl.drawElements(gl.TRIANGLES, mesh.count, gl.UNSIGNED_INT, 0);
      }
      gl.disable(gl.POLYGON_OFFSET_FILL);
      gl.uniform1i(u.screen, 1);
      for (const edge of scene.edges) {
        const projected = project(edge.positions);
        const positions = [];
        const rgb = [];
        const width = edge.width * 96 / 72;
        for (let i = 0; i < projected.length; i += 2) {
          const expanded = screenPolyline(projected.slice(i, i + 2), width, undefined, edge.style, local);
          // An edge drawn on its own opaque faces is read against them, not the
          // plot background: keep the serialised colour (mesh edges on white).
          const raw = edge.colors.subarray(i * 3, i * 3 + 3);
          const ink = edge.faced ? raw : adjust(raw);
          for (let j = 0; j < expanded.length; j += 3) {
            positions.push(expanded[j], expanded[j + 1], expanded[j + 2]);
            rgb.push(...ink);
          }
        }
        slack(width);
        this.pixels(new Float32Array(positions), new Float32Array(rgb));
      }
      for (const line of scene.lines) {
        slack(line.width * 96 / 72);
        this.solid(screenPolyline(project(line.positions), line.width * 96 / 72, line.breaks, line.style, local), adjust(line.color));
      }
      for (const points of scene.points) {
        const sizes = Array.from(points.sizes, (size) => (points.sizeUnits === "points-squared" ? Math.sqrt(size) : size) * 96 / 72);
        const projected = project(points.positions);
        // Individual quads preserve each marker's size and edge thickness.
        gl.uniform1i(u.marker, MARKERS[points.marker]);
        gl.uniform1i(u.hasFace, points.markerFaceAuto || points.faceColor !== "none");
        gl.uniform1i(u.hasEdge, points.edgeColor !== "none");
        gl.uniform3fv(u.faceInk, points.markerFaceAuto ? adjust(palette.background) : points.faceColor === "none" ? palette.text : adjust(points.faceColor));
        gl.uniform3fv(u.edgeInk, points.edgeColor === "none" ? palette.text : adjust(points.edgeColor));
        for (let i = 0; i < sizes.length; i++) {
          const p = projected[i];
          const centre = points.positions.subarray(i * 3, i * 3 + 3);
          // Float32 centres against float64 limits: a marker exactly on a limit stays.
          if (centre.some((v, k) => v < scene.bounds.min[k] - 0.00001 || v > scene.bounds.max[k] + 0.00001)) continue;
          const expanded = geometry.expandMarkers([p], sizes[i]);
          gl.uniform1f(u.border, Math.min(1, 2 * 96 / 72 / Math.max(1, sizes[i])));
          // Clip marker centres, not their pixel offsets, to retain edge glyphs.
          gl.uniform3fv(u.origin, centre);
          gl.uniform3fv(u.pixelX, [0, 0, 0]);
          gl.uniform3fv(u.pixelY, [0, 0, 0]);
          gl.uniform3fv(u.depthVector, [0, 0, 0]);
          this.solidMarker(expanded);
        }
      }
      gl.uniform1i(u.marker, 0);
    });
    gl.disable(gl.SCISSOR_TEST);
    gl.bindVertexArray(null);
  }
  solidMarker(expanded) {
    this.pixels(expanded.positions, new Float32Array(expanded.positions.length), expanded.uv);
  }
  // Read immediately after a fresh draw, without preserveDrawingBuffer.
  readPixels(x, y, dpr) {
    if (this.disposed) return null;
    const rgba = new Uint8Array(4);
    this.gl.readPixels(Math.floor(x * dpr), this.canvas.height - 1 - Math.floor(y * dpr), 1, 1, this.gl.RGBA, this.gl.UNSIGNED_BYTE, rgba);
    return Array.from(rgba);
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    this.frame = null;
    this.canvas.removeEventListener("webglcontextlost", this.lost);
    const gl = this.gl;
    if (gl) {
      this.buffers.forEach((buffer) => gl.deleteBuffer(buffer));
      this.vaos.forEach((vao) => gl.deleteVertexArray(vao));
      if (this.program) gl.deleteProgram(this.program);
      this.shaders.forEach((shader) => gl.deleteShader(shader));
      if (!gl.isContextLost()) gl.getExtension("WEBGL_lose_context")?.loseContext();
    }
    this.buffers = [];
    this.vaos = [];
    this.shaders = [];
    this.meshes = [];
    this.dynamic = null;
    this.program = null;
    this.gl = null;
    if (activeRenderer === this) activeRenderer = null;
  }
}
