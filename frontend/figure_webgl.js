import cameraMath from "./figure_camera_utils.cjs";
import colors from "./figure_color_utils.cjs";
import peeling from "./figure_peeling.cjs";

// Contained policy-dependent shader path. B (scalar -> fragment colormap)
// would add its varying/lookup here and its static scalar buffer/map texture
// in upload(); the default A reuses the existing static RGB attribute.
export function surfaceColorShaderPath(mode) {
  if (mode !== "vertex-rgb") throw new Error("Unsupported surface color interpolation: " + mode);
  return "vec3 surfaceColor(vec3 vertexRGB) { return vertexRGB; }";
}
// One program handles world triangles and R1's pixel-space triangles. All
// positions retain a world coordinate for clipping against the axes box.
const VERTEX = `#version 300 es
precision highp float;
layout(location=0) in vec3 position;
layout(location=1) in vec3 color;
layout(location=2) in vec2 uv;
layout(location=3) in vec4 markerData;
uniform mat4 matrix;
uniform bool screen;
uniform highp int marker;
uniform vec2 viewport;
uniform vec3 origin, pixelX, pixelY, depthVector;
out vec3 world;
smooth out vec3 ink;
out vec2 markerUV;
flat out float markerBorder;
void main() {
  world = marker > 0 ? markerData.xyz : screen ? origin + position.x*pixelX + position.y*pixelY + position.z*depthVector : position;
  gl_Position = screen ? vec4(position.x*2.0/viewport.x-1.0, 1.0-position.y*2.0/viewport.y, position.z*2.0-1.0, 1.0) : matrix*vec4(position,1.0);
  ink = color;
  markerUV = uv;
  markerBorder = markerData.w;
}`;
const FRAGMENT = `#version 300 es
precision highp float;
in vec3 world;
smooth in vec3 ink;
in vec2 markerUV;
flat in float markerBorder;
uniform vec3 low, high, faceInk, edgeInk;
uniform bool clip;
uniform float slack;
uniform highp int marker;
uniform bool hasFace, hasEdge;
out vec4 outputColor;
${surfaceColorShaderPath(colors.SURFACE_COLOR_INTERPOLATION)}
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
  vec3 rgb = surfaceColor(ink);
  if (marker > 0) {
    vec2 p = markerUV;
    if (marker >= 8) {
      float crossLine = min(abs(p.x),abs(p.y));
      float diagonal = min(abs(p.x-p.y),abs(p.x+p.y))*0.7071068;
      float d = marker == 8 ? crossLine : marker == 9 ? diagonal : min(crossLine,diagonal);
      if (!hasEdge || d > markerBorder) discard;
      rgb = edgeInk;
    } else {
      float d = shape(p);
      if (d > 1.0) discard;
      if (hasEdge && (d > 1.0-markerBorder || marker == 7)) rgb = edgeInk;
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
  const output = [];
  appendScreenPolyline(projected, width, breaks, style, viewport, output);
  return new Float32Array(output);
}
function appendScreenPolyline(projected, width, breaks, style, viewport, output, endpointColors, outputColors, endpointAlphas, outputAlphas) {
  const pattern = { "--": [7, 4], ":": [2, 3], "-.": [7, 3, 2, 3] }[style];
  if (style === "none" || !(width > 0)) return;
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
        const p = lerp(start);
        const q = lerp(end);
        if (![p.x, p.y, p.depth, q.x, q.y, q.depth].every(Number.isFinite)) return;
        const sx = q.x - p.x;
        const sy = q.y - p.y;
        const distance = Math.hypot(sx, sy);
        if (!distance) return;
        const nx = -sy / distance * width / 2;
        const ny = sx / distance * width / 2;
        // Same corner order and arithmetic as expandPolyline. Convert to
        // Float32 once at upload, rather than allocating a buffer per dash.
        output.push(p.x + nx, p.y + ny, p.depth, p.x - nx, p.y - ny, p.depth,
          q.x + nx, q.y + ny, q.depth, q.x + nx, q.y + ny, q.depth,
          p.x - nx, p.y - ny, p.depth, q.x - nx, q.y - ny, q.depth);
        if (endpointAlphas) {
          const alphaAt = t => endpointAlphas[i - 1] + (endpointAlphas[i] - endpointAlphas[i - 1]) * t;
          const pAlpha = alphaAt(start), qAlpha = alphaAt(end);
          outputAlphas.push(pAlpha, pAlpha, qAlpha, qAlpha, pAlpha, qAlpha);
        }
        if (endpointColors) {
          const aRGB = endpointColors[i - 1], bRGB = endpointColors[i];
          const rgbAt = t => aRGB.map((v, k) => v + (bRGB[k] - v) * t);
          const pRGB = rgbAt(start), qRGB = rgbAt(end);
          outputColors.push(...pRGB, ...pRGB, ...qRGB, ...qRGB, ...pRGB, ...qRGB);
        }
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
}

export class FigureWebGL {
  constructor(canvas, scenes, failed) {
    activeRenderer?.dispose();
    this.canvas = canvas;
    this.failed = failed;
    this.transparent = scenes.some(scene => scene.hasTransparency);
    this.pending = false;
    this.peeling = null;
    this.transparencyDiagnostics = { enabled: this.transparent, pending: false, limit: peeling.LIMITS.layers };
    this.buffers = [];
    this.vaos = [];
    this.shaders = [];
    this.program = null;
    this.frame = null;
    this.disposed = false;
    this.lost = (event) => {
      event.preventDefault();
      this.fail(this.transparent ? "transparency_unavailable" : "context_lost");
    };
    canvas.addEventListener("webglcontextlost", this.lost);
    try {
      this.gl = canvas.getContext("webgl2", { alpha: false, antialias: true });
    } catch {
      this.gl = null;
    }
    if (!this.gl) {
      this.dispose();
      const code = this.transparent ? "transparency_unavailable" : "webgl_unavailable";
      throw Object.assign(new Error(code), { reason_code: code });
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
      this.uniforms = Object.fromEntries(["matrix", "screen", "viewport", "origin", "pixelX", "pixelY", "depthVector", "low", "high", "marker", "faceInk", "edgeInk", "hasFace", "hasEdge", "clip", "slack"].map((name) => [name, this.gl.getUniformLocation(this.program, name)]));
      this.meshes = scenes.map((scene) => scene.meshes.map((mesh) => this.upload(mesh)));
      this.dynamic = this.upload({ positions: new Float32Array(), colors: new Float32Array(), markerData: new Float32Array() });
      this.zeroUV = new Float32Array();
      this.zeroRGB = new Float32Array();
      this.solidRGB = new Float32Array();
      this.markerPositions = new Float32Array();
      this.markerData = new Float32Array();
      this.markerUV = new Float32Array();
    } catch {
      this.dispose();
      const code = this.transparent ? "transparency_unavailable" : "shader_failure";
      throw Object.assign(new Error(code), { reason_code: code });
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
    if (this.transparent) mesh = peeling.expandMesh(mesh);
    const vao = gl.createVertexArray();
    if (!vao) throw new Error("vertex array");
    this.vaos.push(vao);
    gl.bindVertexArray(vao);
    const values = [mesh.positions, mesh.colors, mesh.uv || new Float32Array(mesh.positions.length / 3 * 2)];
    if (mesh.markerData) values.push(mesh.markerData);
    const attributes = values.map((values, index) => {
      const buffer = this.buffer(gl.ARRAY_BUFFER, values, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(index);
      gl.vertexAttribPointer(index, index === 3 ? 4 : index === 2 ? 2 : 3, gl.FLOAT, false, 0, 0);
      return buffer;
    });
    if (this.transparent) {
      // Attribute 3 remains the marker slot even on ordinary meshes.
      if (!mesh.markerData) attributes[3] = null;
      attributes[4] = this.buffer(gl.ARRAY_BUFFER, mesh.alphas || new Float32Array(), gl.STATIC_DRAW);
      gl.vertexAttribPointer(4, 1, gl.FLOAT, false, 0, 0);
      if (mesh.alphas) gl.enableVertexAttribArray(4);
      else gl.disableVertexAttribArray(4);
    }
    const indices = mesh.indices ? this.buffer(gl.ELEMENT_ARRAY_BUFFER, mesh.indices, gl.STATIC_DRAW) : null;
    return { vao, attributes, indices, count: mesh.indices?.length || mesh.positions.length / 3 };
  }
  fail(code, args = {}) {
    if (this.disposed) return;
    this.pending = false;
    Object.assign(this.transparencyDiagnostics, { pending: false, reason_code: code, reason_args: args });
    this.dispose();
    this.failed(code, args);
  }
  requestRender(draw) {
    if (this.disposed || this.frame !== null) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      if (!this.disposed) draw();
    });
  }
  // Pixel geometry reuses four buffers; no resource accumulation per gesture.
  pixels(positions, rgb, uv, markerData, alphas) {
    if (!positions.length) return;
    const gl = this.gl;
    const uvLength = positions.length / 3 * 2;
    if (!uv && this.zeroUV.length < uvLength) this.zeroUV = new Float32Array(uvLength);
    const values = [positions, rgb, uv || this.zeroUV.subarray(0, uvLength)];
    if (markerData) values.push(markerData);
    gl.bindVertexArray(this.dynamic.vao);
    // An enabled attribute must have enough data even when its shader branch
    // is unused. Lines/grid have no marker data and use the generic default.
    if (markerData) gl.enableVertexAttribArray(3);
    else gl.disableVertexAttribArray(3);
    values.forEach((value, index) => {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.dynamic.attributes[index]);
      gl.bufferData(gl.ARRAY_BUFFER, value, gl.DYNAMIC_DRAW);
    });
    if (this.transparent) {
      if (alphas) {
        gl.enableVertexAttribArray(4);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.dynamic.attributes[4]);
        gl.bufferData(gl.ARRAY_BUFFER, alphas, gl.DYNAMIC_DRAW);
      } else gl.disableVertexAttribArray(4);
    }
    gl.drawArrays(gl.TRIANGLES, 0, positions.length / 3);
  }
  solid(positions, rgb) {
    if (this.solidRGB.length < positions.length) this.solidRGB = new Float32Array(positions.length);
    const values = this.solidRGB.subarray(0, positions.length);
    for (let i = 0; i < values.length; i += 3) values.set(rgb, i);
    this.pixels(positions, values);
  }
  render(frames, palette, dpr) {
    if (this.disposed) return;
    if (!this.transparent) return this.renderOpaque(frames, palette, dpr);
    const started = performance.now();
    try {
      if (!this.peeling) this.peeling = new peeling.Peeling(this.gl, peeling.shaderSources(VERTEX, FRAGMENT), {
        pending: () => {
          this.pending = true;
          this.transparencyDiagnostics.pending = true;
          this.canvas.dispatchEvent(new CustomEvent("figuretransparencypending"));
        },
        fail: (code, args) => this.fail(code, args),
        ready: diagnostics => {
          if (this.disposed) return;
          this.pending = false;
          Object.assign(this.transparencyDiagnostics, diagnostics, { pending: false });
          this.canvas.dispatchEvent(new CustomEvent("figuretransparencyready"));
        }
      });
      const gl = this.gl;
      const groups = peeling.capture(gl, this.uniforms, this.dynamic, proxy => {
        this.gl = proxy;
        try { this.renderOpaque(frames, palette, dpr); }
        finally { this.gl = gl; }
      });
      this.peeling.render(groups, this.canvas.width, this.canvas.height, palette.background, started);
    } catch {
      this.fail("transparency_unavailable");
    }
  }
  // Local deterministic acceptance hook: exercise the same completion policy
  // and normal whole-figure fallback without relying on a slow machine.
  testTransparencyTiming(samples) {
    if (!this.transparent || this.disposed || !Array.isArray(samples) || samples.length > 16 ||
        samples.some(ms => !Number.isFinite(ms) || ms < 0)) return false;
    const timing = new peeling.TransparencyTiming();
    for (const ms of samples) {
      const result = timing.record(ms);
      if (result.failure) { this.fail("transparency_time", result.failure); return true; }
    }
    return false;
  }
  renderOpaque(frames, palette, dpr) {
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
      const projectedBuffers = new Map();
      const project = (positions, vertexIndices) => {
        if (projectedBuffers.has(positions)) return projectedBuffers.get(positions);
        // Grid edges repeat each source vertex up to four times. Their
        // normalised Float32 coordinates are identical, so project once.
        const vertices = vertexIndices ? new Map() : null;
        const projected = Array.from({ length: positions.length / 3 }, (_, i) => {
          const index = vertexIndices?.[i];
          if (vertices?.has(index)) return vertices.get(index);
          const pixel = cameraMath.project(camera, local, positions.subarray(i * 3, i * 3 + 3));
          vertices?.set(index, pixel);
          return pixel;
        });
        projectedBuffers.set(positions, projected);
        return projected;
      };
      const contourColors = colors.contourDisplayColors(scene.axes, palette.background, palette.text);
      const adjust = (rgb) => colors.adjustForContrast(rgb, palette.background, palette.text);
      // A pixel-wide stroke lying on a limit plane leaves the box by half its
      // width; allow exactly that much so boundary data is not cut in half.
      const unit = Math.hypot(...pixelX);
      const slack = (pixels) => gl.uniform1f(u.slack, 0.0001 + unit * pixels / 2);
      if (scene.axes && scene.axes.visible !== false) {
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
      const drawEdge = (edge) => {
        gl.uniform1i(u.screen, 1);
        const projected = project(edge.positions, edge.source.vertexIndices);
        const positions = [];
        const rgb = [];
        const alphas = edge.alphas ? [] : null;
        const width = edge.width * 96 / 72;
        const interpolated = edge.colorInterpolation === "vertex-rgb";
        const endpointColors = interpolated ? projected.map((_, i) => {
          const rgb = Array.from(edge.colors.subarray(i * 3, i * 3 + 3));
          return edge.faced ? rgb : adjust(rgb);
        }) : null;
        for (let i = 0; i < projected.length; i += 2) {
          const start = positions.length;
          // Patch edges retain source polygon order and continuous dash phase.
          // Surface grid edges keep their existing independent-segment policy.
          let end = i + 2;
          if (edge.source.series.kind === "patch3d") {
            while (end < projected.length && edge.source.faceIndices[end] === edge.source.faceIndices[i]) end += 2;
            const path = [projected[i]], inks = interpolated ? [endpointColors[i]] : null;
            const opacity = alphas ? [edge.alphas[i]] : null;
            for (let k = i + 1; k < end; k += 2) {
              path.push(projected[k]);
              if (inks) inks.push(endpointColors[k]);
              if (opacity) opacity.push(edge.alphas[k]);
            }
            appendScreenPolyline(path, width, [0, path.length], edge.style, local, positions, inks, rgb, opacity, alphas);
          } else appendScreenPolyline(projected, width, [i, i + 2], edge.style, local, positions, endpointColors, rgb, edge.alphas, alphas);
          // An edge drawn on its own opaque faces is read against them, not the
          // plot background: keep the serialised colour (mesh edges on white).
          const raw = edge.colors.subarray(i * 3, i * 3 + 3);
          const ink = edge.faced ? raw : adjust(raw);
          if (!interpolated) for (let j = start; j < positions.length; j += 3) rgb.push(...ink);
          i = end - 2;
        }
        slack(width);
        this.pixels(new Float32Array(positions), new Float32Array(rgb), undefined, undefined, alphas ? new Float32Array(alphas) : undefined);
      };
      // Octave contourf patches overpaint earlier regions, including their
      // borders. Keep each fill and border together. Borders test against the
      // offset faces but do not write depth: later coplanar fills overwrite
      // them instead of failing against an earlier border's nearer depth.
      const meshSlots = scene.meshes.map((source, index) => ({ source, mesh: this.meshes[axis][index] }));
      // Native no-area contour children have a border but no fill mesh. Give
      // them their original series slot so subsequent regions overpaint them.
      for (const edge of scene.edges) if (edge.source.series.role === "contourf" &&
        !scene.meshes.some(source => source.series === edge.series)) meshSlots.push({ source: edge });
      meshSlots.sort((a, b) => a.source.series - b.source.series);
      for (const { source, mesh } of meshSlots) {
        const filledContour = source.source.series.role === "contourf";
        if (mesh) {
          gl.enable(gl.POLYGON_OFFSET_FILL);
          gl.polygonOffset(1, 1);
          gl.uniform1i(u.screen, 0);
          gl.uniform1f(u.slack, 0.00001);
          gl.bindVertexArray(mesh.vao);
          gl.drawElements(gl.TRIANGLES, mesh.count, gl.UNSIGNED_INT, 0);
        }
        if (filledContour) {
          gl.disable(gl.POLYGON_OFFSET_FILL);
          gl.depthMask(false);
          for (const edge of scene.edges) if (edge.series === source.series) drawEdge(edge);
          gl.depthMask(true);
        }
      }
      gl.disable(gl.POLYGON_OFFSET_FILL);
      gl.uniform1i(u.screen, 1);
      for (const edge of scene.edges) if (edge.source.series.role !== "contourf") drawEdge(edge);
      for (const line of scene.lines) {
        slack(line.width * 96 / 72);
        this.solid(screenPolyline(project(line.positions), line.width * 96 / 72, line.breaks, line.style, local), contourColors.get(line.source?.series) ?? adjust(line.color));
      }
      for (const points of scene.points) {
        const sizes = Array.from(points.sizes, (size) => (points.sizeUnits === "points-squared" ? Math.sqrt(size) : size) * 96 / 72);
        const projected = project(points.positions);
        // One batch per series; centres and edge thickness travel per vertex.
        gl.uniform1i(u.marker, MARKERS[points.marker]);
        gl.uniform1i(u.hasFace, points.markerFaceAuto || points.faceColor !== "none");
        gl.uniform1i(u.hasEdge, points.edgeColor !== "none");
        gl.uniform3fv(u.faceInk, points.markerFaceAuto ? adjust(palette.background) : points.faceColor === "none" ? palette.text : adjust(points.faceColor));
        gl.uniform3fv(u.edgeInk, points.edgeColor === "none" ? palette.text : adjust(points.edgeColor));
        const capacity = sizes.length * 18;
        if (this.markerPositions.length < capacity) {
          this.markerPositions = new Float32Array(capacity);
          this.markerUV = new Float32Array(sizes.length * 12);
          this.markerData = new Float32Array(sizes.length * 24);
        }
        const corners = [-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1];
        let count = 0;
        for (let i = 0; i < sizes.length; i++) {
          const p = projected[i];
          const centre = points.positions.subarray(i * 3, i * 3 + 3);
          // Clip centres, retaining complete glyphs exactly on axes limits.
          if (centre.some((v, k) => v < scene.bounds.min[k] - 0.00001 || v > scene.bounds.max[k] + 0.00001)) continue;
          const size = sizes[i];
          if (!(size > 0) || ![p.x, p.y, p.depth, size].every(Number.isFinite)) continue;
          const border = Math.min(1, 2 * 96 / 72 / Math.max(1, size));
          for (let vertex = 0; vertex < 6; vertex++) {
            const offset = count * 6 + vertex;
            const uvX = corners[vertex * 2];
            const uvY = corners[vertex * 2 + 1];
            this.markerPositions[offset * 3] = p.x + uvX * size / 2;
            this.markerPositions[offset * 3 + 1] = p.y + uvY * size / 2;
            this.markerPositions[offset * 3 + 2] = p.depth;
            this.markerUV[offset * 2] = uvX;
            this.markerUV[offset * 2 + 1] = uvY;
            this.markerData.set(centre, offset * 4);
            this.markerData[offset * 4 + 3] = border;
          }
          count++;
        }
        this.solidMarker({ positions: this.markerPositions.subarray(0, count * 18),
          uv: this.markerUV.subarray(0, count * 12), markerData: this.markerData.subarray(0, count * 24) });
      }
      gl.uniform1i(u.marker, 0);
    });
    gl.disable(gl.SCISSOR_TEST);
    gl.bindVertexArray(null);
  }
  solidMarker(expanded) {
    if (this.zeroRGB.length < expanded.positions.length) this.zeroRGB = new Float32Array(expanded.positions.length);
    this.pixels(expanded.positions, this.zeroRGB.subarray(0, expanded.positions.length), expanded.uv, expanded.markerData);
  }
  // Opaque callers draw immediately before reading. A certified transparent
  // image remains in the offscreen target after the compositor clears canvas.
  readPixels(x, y, dpr) {
    if (this.disposed || this.pending) return null;
    if (this.transparent && !this.peeling?.frames) return null;
    const gl = this.gl;
    const rgba = new Uint8Array(4);
    const read = () => gl.readPixels(Math.floor(x * dpr), this.canvas.height - 1 - Math.floor(y * dpr), 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
    if (this.transparent) {
      const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING);
      try {
        gl.bindFramebuffer(gl.READ_FRAMEBUFFER, this.peeling.output.fbo);
        read();
      } finally {
        gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous);
      }
    } else read();
    return Array.from(rgba);
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.pending = false;
    this.transparencyDiagnostics.pending = false;
    if (this.frame !== null) cancelAnimationFrame(this.frame);
    this.frame = null;
    this.canvas.removeEventListener("webglcontextlost", this.lost);
    this.peeling?.dispose();
    this.peeling = null;
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
    this.zeroUV = null;
    this.zeroRGB = null;
    this.solidRGB = null;
    this.markerPositions = null;
    this.markerData = null;
    this.markerUV = null;
    this.program = null;
    this.gl = null;
    if (activeRenderer === this) activeRenderer = null;
  }
}
