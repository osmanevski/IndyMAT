"use strict";

// The only admission gate for interactive artifacts. No DOM, engine or globals.
const LIMITS = Object.freeze({ samples: 2000, surface_vertices: 40000,
  patch_vertices: 40000, patch_triangles: 80000, vertices: 100000,
  triangles: 200000, axes: 16, series: 128, colormap: 4096,
  json_bytes: 8388608, objects: 1024, depth: 16, source_samples: 1000000, ticks: 4096, global_objects: 12289 });
const REASON_CODES = Object.freeze(["no_axes", "budget_exceeded", "invalid_data",
  "unsupported_object", "unsupported_group", "unsupported_marker", "unsupported_color",
  "scatter_colors", "transparency", "lighting", "interpolated_color",
  "unsupported_surface", "log_3d", "perspective", "manual_camera", "unsupported_units",
  "unsupported_colorbar", "json_budget", "patch_colors", "unsupported_patch", "unsupported_text"]);
// Added with the 2D shapes/text records; older artifacts do not carry them.
const OPTIONAL_LIMITS = Object.freeze({ text_lines: 256, text_chars: 65536, image_pixels: 262144 });
const STYLES = ["-", "--", ":", "-.", "none"];
const MARKERS = ["none", "o", "s", "square", "^", "v", ">", "<", ".", "+", "x", "*"];
const CLASSES = ["double", "single", "int8", "uint8", "int16", "uint16", "int32", "uint32", "int64", "uint64"];
const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
const record = (o) => o !== null && typeof o === "object" && !Array.isArray(o);
const finite = Number.isFinite;
const integer = (n) => Number.isSafeInteger(n) && n >= 0;
const coordinate = (n) => n === null || finite(n);
const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
const rgb = (v) => Array.isArray(v) && v.length === 3 && v.every((n) => finite(n) && n >= 0 && n <= 1);
const color = (v) => v === "none" || rgb(v);
function failure(code, path, args = {}) {
  const e = new Error(code);
  e.reason_code = code;
  e.path = path;
  e.reason_args = args;
  throw e;
}
function requireThat(ok, path, code = "invalid_data", args) {
  if (!ok) failure(code, path, args);
}
function budget(n, key, path) {
  requireThat(integer(n), path);
  if (n > (LIMITS[key] ?? OPTIONAL_LIMITS[key])) failure("budget_exceeded", path, { budget: key, actual: n, limit: LIMITS[key] ?? OPTIONAL_LIMITS[key] });
}
function numbers(a, n, path, nullable = false) {
  requireThat(Array.isArray(a) && (n === undefined || a.length === n), path);
  requireThat(a.every(nullable ? coordinate : finite), path);
  return a;
}
function text(v, path) {
  requireThat(typeof v === "string", path);
}
function labels(v, path) {
  requireThat(Array.isArray(v) && v.length <= LIMITS.ticks && v.every((s) => typeof s === "string"), path);
}
function range(v, path) {
  numbers(v, 2, path);
  requireThat(v[0] < v[1], path);
}
function descriptor(d, path = "array", cap = LIMITS.vertices * 3, nullable = true) {
  requireThat(record(d) && d.order === "column-major", path);
  requireThat(Array.isArray(d.shape) && d.shape.length >= 2 && d.shape.length <= 3, path + ".shape");
  let count = 1;
  for (const size of d.shape) {
    requireThat(integer(size), path + ".shape");
    count *= size;
    requireThat(Number.isSafeInteger(count) && count <= cap, path + ".shape", "budget_exceeded", { budget: "array_values", actual: count, limit: cap });
  }
  numbers(d.values, count, path + ".values", nullable);
  return d;
}
// Returns a copy; never coerces null into zero or mutates the descriptor.
function image(s, p) {
  const [rows, columns] = s.shape, count = rows * columns;
  for (const [key, size] of [["x", columns], ["y", rows]]) {
    numbers(s[key], 2, p + "." + key);
    requireThat(size === 1 ? s[key][0] === s[key][1] : s[key][0] !== s[key][1], p + "." + key);
    const delta = size > 1 ? (s[key][1] - s[key][0]) / (size - 1) : 1;
    requireThat(Number.isFinite(delta) && Number.isFinite(s[key][0] - delta / 2) && Number.isFinite(s[key][1] + delta / 2), p + "." + key);
  }
  requireThat(["indexed", "truecolor"].includes(s.encoding) && ["scaled", "direct"].includes(s.mapping), p + ".encoding");
  requireThat(["double", "single", "uint8", "uint16"].includes(s.cdata_class), p + ".cdata_class");
  descriptor(s.cdata, p + ".cdata", OPTIONAL_LIMITS.image_pixels * 3, false);
  requireThat(equal(s.cdata.shape, s.encoding === "truecolor" ? [rows, columns, 3] : [rows, columns]), p + ".cdata.shape");
  const maximum = s.cdata_class === "uint8" ? 255 : s.cdata_class === "uint16" ? 65535 : null;
  if (maximum !== null) requireThat(s.cdata.values.every((n) => integer(n) && n <= maximum), p + ".cdata.values");
  requireThat(s.pixel_count === count && s.original_points === 0 && s.rendered_points === 0 && s.decimated === false, p + ".counts");
}

function decodeArray(d) {
  try {
    descriptor(d);
    return { ok: true, shape: d.shape.slice(), order: d.order, values: d.values.slice() };
  } catch (e) {
    return { ok: false, reason_code: e.reason_code || "invalid_data", path: e.path || "array", reason_args: e.reason_args || {} };
  }
}
function classValues(d, name, path) {
  const bounds = { int8: [-128, 127], uint8: [0, 255], int16: [-32768, 32767], uint16: [0, 65535],
    int32: [-2147483648, 2147483647], uint32: [0, 4294967295], int64: [-Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER], uint64: [0, Number.MAX_SAFE_INTEGER] };
  if (own(bounds, name)) requireThat(d.values.every((v) => v === null || Number.isSafeInteger(v) && v >= bounds[name][0] && v <= bounds[name][1]), path);
}
function surfaceColor(c, shape, path, association = "cell") {
  requireThat(record(c), path);
  if (c.mode === "none") {
    requireThat(c.association === "constant", path);
    return;
  }
  if (c.mode === "constant") {
    requireThat(c.association === "constant" && rgb(c.rgb), path);
    return;
  }
  requireThat(c.mode === "flat" && c.association === association, path, "unsupported_color");
  requireThat(["indexed", "truecolor"].includes(c.encoding) && ["scaled", "direct"].includes(c.mapping) && CLASSES.includes(c.cdata_class), path);
  descriptor(c.data, path + ".data", LIMITS.surface_vertices * 3);
  classValues(c.data, c.cdata_class, path + ".data");
  requireThat(equal(c.data.shape, c.encoding === "indexed" ? shape : [...shape, 3]), path + ".data.shape");
  if (c.encoding === "truecolor") {
    const max = c.cdata_class === "uint8" ? 255 : c.cdata_class === "uint16" ? 65535 : 1;
    requireThat(["double", "single", "uint8", "uint16"].includes(c.cdata_class) && c.data.values.every((v) => v === null || v >= 0 && v <= max), path + ".data");
  }
}
function surface(s, path) {
  requireThat(Array.isArray(s.shape) && s.shape.length === 2 && s.shape.every((n) => integer(n) && n >= 2), path + ".shape");
  const [rows, cols] = s.shape;
  budget(rows * cols, "surface_vertices", path);
  requireThat(record(s.coordinate_layout), path + ".coordinate_layout");
  for (const key of ["x", "y", "z"]) {
    descriptor(s[key], path + "." + key, LIMITS.surface_vertices);
    if (key === "z" || s.coordinate_layout[key] === "matrix") requireThat(equal(s[key].shape, s.shape), path + "." + key + ".shape");
    else {
      requireThat(s.coordinate_layout[key] === "vector", path + ".coordinate_layout");
      requireThat(s[key].shape[0] === 1 || s[key].shape[1] === 1, path + "." + key + ".shape");
      requireThat(s[key].values.length === (key === "x" ? cols : rows), path + "." + key);
    }
  }
  requireThat(s.index_base === 0 && s.cell_color_owner === "row-column-origin" && s.edge_color_owner === "first-vertex", path);
  const vertexValid = (i) => {
    const r = i % rows, c = Math.floor(i / rows);
    return s.z.values[i] !== null && s.x.values[s.coordinate_layout.x === "vector" ? c : i] !== null && s.y.values[s.coordinate_layout.y === "vector" ? r : i] !== null;
  };
  const expectedCells = [];
  for (let c = 0; c < cols - 1; c++) {
    for (let r = 0; r < rows - 1; r++) {
      const i = r + c * rows;
      if ([i, i + 1, i + rows, i + rows + 1].every(vertexValid)) expectedCells.push(i);
    }
  }
  requireThat(Array.isArray(s.cell_origins) && equal(s.cell_origins, expectedCells), path + ".cell_origins", "invalid_index");
  descriptor(s.edge_indices, path + ".edge_indices", LIMITS.surface_vertices * 4, false);
  requireThat(s.edge_indices.shape[1] === 2, path + ".edge_indices.shape");
  requireThat(["both", "row", "column"].includes(s.mesh_style), path + ".mesh_style");
  const edges = [];
  if (s.mesh_style !== "row") {
    for (let c = 0; c < cols; c++) for (let r = 0; r < rows - 1; r++) {
      const i = r + c * rows;
      if (vertexValid(i) && vertexValid(i + 1)) edges.push([i, i + 1]);
    }
  }
  if (s.mesh_style !== "column") {
    for (let c = 0; c < cols - 1; c++) for (let r = 0; r < rows; r++) {
      const i = r + c * rows;
      if (vertexValid(i) && vertexValid(i + rows)) edges.push([i, i + rows]);
    }
  }
  requireThat(s.edge_indices.shape[0] === edges.length && equal(s.edge_indices.values, [...edges.map((e) => e[0]), ...edges.map((e) => e[1])]), path + ".edge_indices", "invalid_index");
  requireThat(s.triangle_count === expectedCells.length * 2 && s.original_points === rows * cols && s.rendered_points === rows * cols && s.decimated === false, path + ".counts");
  requireThat(CLASSES.includes(s.cdata_class) && ["scaled", "direct"].includes(s.cdata_mapping), path + ".cdata_class");
  descriptor(s.cdata, path + ".cdata", LIMITS.surface_vertices * 3);
  classValues(s.cdata, s.cdata_class, path + ".cdata");
  requireThat(equal(s.cdata.shape, s.shape) || equal(s.cdata.shape, [...s.shape, 3]), path + ".cdata.shape");
  for (const field of ["face_color", "edge_color"]) {
    const color = s[field];
    if (record(color) && color.mode === "flat") {
      requireThat(color.cdata_class === s.cdata_class && color.mapping === s.cdata_mapping && record(color.data) && Array.isArray(color.data.shape) && Array.isArray(color.data.values) && equal(color.data.shape, s.cdata.shape) && equal(color.data.values, s.cdata.values), path + "." + field + ".data");
      const owners = field === "face_color" ? expectedCells : edges.map((e) => e[0]);
      for (const i of owners) for (let channel = 0; channel < (color.encoding === "truecolor" ? 3 : 1); channel++) requireThat(s.cdata.values[i + channel * rows * cols] !== null, path + "." + field + ".data");
    }
  }
  surfaceColor(s.face_color, s.shape, path + ".face_color");
  surfaceColor(s.edge_color, s.shape, path + ".edge_color", "vertex");
}
function patch2d(s, path) {
  requireThat(["patch", "bar", "area", "textbox"].includes(s.role) && s.index_base === 0, path + ".role");
  descriptor(s.vertices, path + ".vertices", LIMITS.patch_vertices * 2);
  requireThat(s.vertices.shape.length === 2 && s.vertices.shape[1] === 2 && s.vertices.shape[0] > 0, path + ".vertices.shape");
  const n = s.vertices.shape[0];
  budget(n, "patch_vertices", path + ".vertices");
  requireThat(record(s.faces) && Array.isArray(s.faces.shape) && s.faces.shape.length === 2 && s.faces.shape.every(integer), path + ".faces.shape");
  const [rows, cols] = s.faces.shape;
  requireThat(rows > 0 && cols > 0, path + ".faces.shape");
  budget(rows * Math.max(0, cols - 2), "patch_triangles", path + ".faces");
  descriptor(s.faces, path + ".faces", LIMITS.patch_triangles * 3);
  // Zero-based vertex indices; null is only the trailing padding of a face.
  for (let r = 0; r < rows; r++) {
    let ended = false;
    for (let c = 0; c < cols; c++) {
      const v = s.faces.values[r + c * rows];
      if (v === null) ended = true;
      else requireThat(!ended && integer(v) && v < n, path + ".faces", "invalid_index");
    }
  }
  requireThat(color(s.face_color) && color(s.edge_color), path + ".color", "unsupported_color");
  requireThat(finite(s.line_width) && s.line_width >= 0, path + ".line_width");
  requireThat(s.decimated === false && s.original_points === n && s.rendered_points === n, path + ".counts");
  if (own(s, "bar")) {
    const b = s.bar;
    requireThat(s.role === "bar" && record(b) && typeof b.horizontal === "boolean" && typeof b.layout === "string" && finite(b.width) && finite(b.base_value), path + ".bar");
    numbers(b.positions, rows, path + ".bar.positions", true);
    numbers(b.values, rows, path + ".bar.values", true);
  }
}
function textRecord(s, path) {
  requireThat(typeof s.role === "string" && ["data", "normalized"].includes(s.units), path + ".units", "unsupported_text", { property: "units" });
  numbers(s.position, 2, path + ".position");
  if (own(s, "figure_title")) requireThat(typeof s.figure_title === "boolean", path + ".figure_title");
  requireThat(Array.isArray(s.lines) && s.lines.every((line) => typeof line === "string"), path + ".lines");
  if (s.lines.length > OPTIONAL_LIMITS.text_lines) failure("budget_exceeded", path + ".lines", { budget: "text_lines", actual: s.lines.length, limit: OPTIONAL_LIMITS.text_lines });
  const chars = s.lines.reduce((sum, line) => sum + line.length, 0);
  if (chars > OPTIONAL_LIMITS.text_chars) failure("budget_exceeded", path + ".lines", { budget: "text_chars", actual: chars, limit: OPTIONAL_LIMITS.text_chars });
  requireThat(["tex", "none"].includes(s.interpreter), path + ".interpreter", "unsupported_text", { property: "interpreter" });
  requireThat(["left", "center", "right"].includes(s.horizontal_alignment) && ["top", "cap", "middle", "baseline", "bottom"].includes(s.vertical_alignment), path + ".alignment");
  requireThat(finite(s.rotation) && s.rotation >= 0 && s.rotation < 360, path + ".rotation");
  requireThat(finite(s.font_size) && s.font_size > 0 && finite(s.margin) && s.margin >= 0 && finite(s.line_width) && s.line_width >= 0, path + ".font_size");
  requireThat(["normal", "bold"].includes(s.font_weight) && ["normal", "italic"].includes(s.font_angle) && typeof s.clipping === "boolean", path + ".font");
  requireThat(rgb(s.color) && color(s.background_color) && color(s.edge_color), path + ".color", "unsupported_color");
  requireThat(s.decimated === false && s.original_points === 0 && s.rendered_points === 0, path + ".counts");
}
function samples(s, version, path) {
  requireThat(["line", "scatter", "stem", "stairs"].includes(s.kind), path + ".kind", "unsupported_object");
  if (own(s, "span")) requireThat(version === 3 && s.kind === "line" && ["horizontal", "vertical"].includes(s.span) && s.x.length === 2, path + ".span");
  if (own(s, "role")) requireThat(version === 3 && typeof s.role === "string", path + ".role");
  numbers(s.x, undefined, path + ".x", true);
  budget(s.x.length, "samples", path);
  numbers(s.y, s.x.length, path + ".y", true);
  if (own(s, "z")) {
    requireThat(version === 3 && ["line", "scatter"].includes(s.kind), path + ".z");
    numbers(s.z, s.x.length, path + ".z", true);
  }
  requireThat(integer(s.original_points) && s.original_points >= s.x.length && typeof s.decimated === "boolean", path + ".counts");
  if (version === 3) budget(s.original_points, "source_samples", path + ".original_points");
  if (version === 3 || own(s, "source_indices")) {
    requireThat(s.rendered_points === s.x.length, path + ".rendered_points");
    requireThat(Array.isArray(s.source_indices) && s.source_indices.length === s.x.length, path + ".source_indices", "invalid_index");
    requireThat(s.source_indices.every((v, i, a) => integer(v) && v < s.original_points && (i === 0 || a[i - 1] < v)), path + ".source_indices", "invalid_index");
    requireThat(s.decimated === (s.x.length < s.original_points), path + ".decimated");
    requireThat(finite(s.line_width) && s.line_width >= 0 && finite(s.marker_size) && s.marker_size >= 0, path + ".size");
  }
  requireThat(MARKERS.includes(s.marker), path + ".marker", "unsupported_marker");
  requireThat(color(s.line_color) && color(s.marker_edge_color) && color(s.marker_face_color), path + ".color", "unsupported_color");
  requireThat(typeof s.marker_face_auto === "boolean", path + ".marker_face_auto");
  if (version === 3 && s.kind === "scatter") {
    if (own(s, "z")) {
      const c = s.color_data;
      requireThat(record(c) && CLASSES.includes(c.cdata_class) && ["indexed", "truecolor"].includes(c.encoding) && c.mapping === "scaled", path + ".color_data");
      descriptor(c.data, path + ".color_data.data", 3, false);
      classValues(c.data, c.cdata_class, path + ".color_data.data");
      requireThat(c.data.values.length === (c.encoding === "indexed" ? 1 : 3), path + ".color_data");
    }
    requireThat(Array.isArray(s.sizes) && (s.sizes.length === 1 || s.sizes.length === s.x.length), path + ".sizes");
    requireThat(s.sizes.every((v) => finite(v) && v >= 0) && s.size_units === "points-squared", path + ".sizes");
  }
}
function camera(a, path) {
  const c = a.camera;
  requireThat(record(c) && c.projection === "orthographic", path, "perspective");
  numbers(a.view, 2, path + ".view");
  for (const key of ["position", "target", "up_vector"]) numbers(c[key], 3, path + "." + key);
  requireThat(finite(c.view_angle) && c.view_angle > 0 && c.view_angle < 180, path + ".view_angle");
  requireThat(c.position.some((v, i) => v !== c.target[i]) && c.up_vector.some((v) => v !== 0), path);
  const direction = c.position.map((v, i) => v - c.target[i]);
  const cross = [direction[1] * c.up_vector[2] - direction[2] * c.up_vector[1], direction[2] * c.up_vector[0] - direction[0] * c.up_vector[2], direction[0] * c.up_vector[1] - direction[1] * c.up_vector[0]];
  requireThat(cross.every(finite) && cross.some((v) => v !== 0), path + ".up_vector");
  for (const key of ["position_mode", "target_mode", "upvector_mode", "viewangle_mode"]) requireThat(c[key] === "auto", path + "." + key, "manual_camera");
  for (const key of ["data_aspect_ratio", "plot_box_aspect_ratio"]) {
    numbers(a[key], 3, path + "." + key);
    requireThat(a[key].every((v) => v > 0) && ["auto", "manual"].includes(a[key + "_mode"]), path + "." + key);
  }
}
function colormap(d, path) {
  descriptor(d, path, LIMITS.colormap * 3, false);
  requireThat(d.shape.length === 2 && d.shape[1] === 3 && d.shape[0] > 0 && d.shape[0] <= LIMITS.colormap && d.values.every((v) => v >= 0 && v <= 1), path);
}
// Count encoded UTF-8 bytes without constructing a second, possibly huge JSON string.
function jsonBudget(value) {
  let bytes = 0;
  const ancestors = new Set();
  const add = (n) => {
    bytes += n;
    if (bytes > LIMITS.json_bytes) failure("json_budget", "figure", { actual: bytes, limit: LIMITS.json_bytes });
  };
  const stringBytes = (s) => {
    add(2);
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i);
      if (c === 34 || c === 92 || [8, 9, 10, 12, 13].includes(c)) add(2);
      else if (c < 32) add(6);
      else if (c < 128) add(1);
      else if (c < 2048) add(2);
      else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length && s.charCodeAt(i + 1) >= 0xdc00 && s.charCodeAt(i + 1) <= 0xdfff) { add(4); i++; }
      else if (c >= 0xd800 && c <= 0xdfff) add(6);
      else add(3);
    }
  };
  const visit = (v, depth) => {
    requireThat(depth <= 24, "figure.depth");
    if (v === null) { add(4); return; }
    if (typeof v === "string") { stringBytes(v); return; }
    if (typeof v === "boolean") { add(v ? 4 : 5); return; }
    if (typeof v === "number") { requireThat(finite(v), "figure.number"); add(String(v).length); return; }
    requireThat(typeof v === "object" && !ancestors.has(v), "figure.object");
    requireThat(Array.isArray(v) || Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null, "figure.object");
    ancestors.add(v);
    add(2);
    if (Array.isArray(v)) {
      requireThat(v.length <= LIMITS.json_bytes / 2, "figure.array", "json_budget");
      for (let i = 0; i < v.length; i++) { if (i) add(1); visit(v[i], depth + 1); }
    } else {
      let index = 0;
      for (const key of Object.keys(v)) {
        if (index++) add(1);
        stringBytes(key);
        add(1);
        visit(v[key], depth + 1);
      }
    }
    ancestors.delete(v);
  };
  visit(value, 0);
  return bytes;
}
function legacyArrays(payload) {
  // Old jsonencode collapsed singleton numeric vectors. This only normalises
  // v2 vector fields; matrices in v3 must always have explicit descriptors.
  const vector = (v) => v === null || finite(v) ? [v] : v;
  return { ...payload, axes: payload.axes.map((a) => record(a) && Array.isArray(a.series) ?
    { ...a, series: a.series.map((s) => record(s) ? { ...s, x: vector(s.x), y: vector(s.y) } : s) } : a) };
}
function validate(payload) {
  try {
    requireThat(record(payload), "figure");
    requireThat([2, 3].includes(payload.version), "version", "unknown_version");
    requireThat(typeof payload.supported === "boolean" && typeof payload.decimated === "boolean", "figure.flags");
    requireThat(Array.isArray(payload.axes), "axes");
    budget(payload.axes.length, "axes", "axes");
    jsonBudget(payload);
    if (record(payload.source) && own(payload.source, "figure_size")) {
      numbers(payload.source.figure_size, 2, "source.figure_size");
      requireThat(payload.source.figure_size.every((n) => n > 0), "source.figure_size");
    }
    if (payload.version === 2) payload = legacyArrays(payload);
    if (!payload.supported) {
      if (payload.version === 3 || own(payload, "reason_code")) requireThat(REASON_CODES.includes(payload.reason_code) && record(payload.reason_args), "reason_code");
      if (payload.version === 3 || own(payload, "reason_code")) requireThat(payload.axes.length === 0, "axes");
      return { ok: true, supported: false, reason_code: payload.reason_code || "unsupported_object", reason_args: payload.reason_args || {}, data: payload };
    }
    requireThat(payload.axes.length > 0, "axes", "no_axes");
    if (payload.version === 3) {
      requireThat(record(payload.source) && typeof payload.source.job === "string" && (payload.source.job === "" || /^[a-f0-9]{32}$/.test(payload.source.job)) && integer(payload.source.figure), "source");
      requireThat(record(payload.limits), "limits");
      for (const [key, value] of Object.entries(LIMITS)) requireThat(payload.limits[key] === value, "limits." + key);
      for (const [key, value] of Object.entries(OPTIONAL_LIMITS)) requireThat(!own(payload.limits, key) || payload.limits[key] === value, "limits." + key);
      requireThat(payload.reason_code === "" && record(payload.reason_args), "reason_code");
    }
    // Reject aggregate upper bounds before constructing expected cells/edges.
    let estimatedVertices = 0, estimatedTriangles = 0, estimatedSeries = 0, estimatedPixels = 0;
    for (const a of payload.axes) {
      requireThat(record(a) && Array.isArray(a.series), "axes.series");
      estimatedSeries += a.series.length;
      budget(estimatedSeries, "series", "series");
      for (const s of a.series) {
        requireThat(record(s), "series");
        if (s.kind === "surface") {
          requireThat(Array.isArray(s.shape) && s.shape.length === 2 && s.shape.every((n) => integer(n) && n >= 2), "surface.shape");
          const n = s.shape[0] * s.shape[1];
          budget(n, "surface_vertices", "surface.shape");
          estimatedVertices += n;
          estimatedTriangles += 2 * (s.shape[0] - 1) * (s.shape[1] - 1);
        } else if (s.kind === "image") {
          requireThat(Array.isArray(s.shape) && s.shape.length === 2 && s.shape.every((n) => integer(n) && n > 0), "image.shape");
          estimatedPixels += s.shape[0] * s.shape[1];
          budget(estimatedPixels, "image_pixels", "image.shape");
        } else if (s.kind === "patch2d") {
          requireThat(record(s.vertices) && Array.isArray(s.vertices.shape) && integer(s.vertices.shape[0]), "patch2d.vertices");
          budget(s.vertices.shape[0], "patch_vertices", "patch2d.vertices");
          estimatedVertices += s.vertices.shape[0];
        } else if (s.kind !== "text") {
          requireThat(Array.isArray(s.x), "series.x");
          budget(s.x.length, "samples", "series.x");
          estimatedVertices += s.x.length;
        }
        budget(estimatedVertices, "vertices", "vertex_count");
        budget(estimatedTriangles, "triangles", "triangle_count");
      }
    }
    let vertices = 0, triangles = 0, pixels = 0, seriesCount = 0, reduced = false;
    for (let i = 0; i < payload.axes.length; i++) {
      const a = payload.axes[i], path = "axes[" + i + "]";
      requireThat(record(a) && a.supported === true && Array.isArray(a.series), path);
      numbers(a.position, 4, path + ".position");
      if (own(a, "title_layout_position")) {
        numbers(a.title_layout_position, 4, path + ".title_layout_position");
        requireThat(a.title_layout_position[2] > 0 && a.title_layout_position[3] > 0, path + ".title_layout_position");
      }
      requireThat(a.position[2] > 0 && a.position[3] > 0, path + ".position");
      const dimension = payload.version === 2 ? 2 : a.dimension;
      requireThat([2, 3].includes(dimension), path + ".dimension");
      for (const key of dimension === 3 ? ["x", "y", "z"] : ["x", "y"]) {
        range(a[key + "lim"], path + "." + key + "lim");
        requireThat(["linear", "log"].includes(a[key + "scale"]) && ["normal", "reverse"].includes(a[key + "dir"]), path + "." + key);
        if (dimension === 3) requireThat(a[key + "scale"] === "linear", path + "." + key, "log_3d");
      }
      requireThat(record(a.grid) && typeof a.grid.x === "boolean" && typeof a.grid.y === "boolean", path + ".grid");
      requireThat(record(a.legend) && typeof a.legend.visible === "boolean", path + ".legend");
      labels(a.legend.labels, path + ".legend.labels");
      for (const key of ["title", "xlabel", "ylabel"]) text(a[key], path + "." + key);
      if (own(a, "visible")) requireThat(typeof a.visible === "boolean", path + ".visible");
      if (own(a, "interpreters")) requireThat(record(a.interpreters) && ["title", "xlabel", "ylabel", "ticks"].every((key) => typeof a.interpreters[key] === "string"), path + ".interpreters");
      for (const key of ["xtickmode", "ytickmode", "xticklabelmode", "yticklabelmode"]) if (own(a, key)) requireThat(["auto", "manual"].includes(a[key]), path + "." + key);
      if (payload.version === 3) {
        requireThat(a.id === i, path + ".id", "invalid_index");
        range(a.clim, path + ".clim");
        colormap(a.colormap, path + ".colormap");
        if (dimension === 3) camera(a, path + ".camera");
        for (const key of ["x", "y", "z"]) {
          numbers(a[key + "tick"], undefined, path + "." + key + "tick");
          budget(a[key + "tick"].length, "ticks", path);
          labels(a[key + "ticklabel"], path + "." + key + "ticklabel");
        }
        requireThat(Array.isArray(a.colorbars) && a.colorbars.length <= LIMITS.axes, path + ".colorbars");
        for (const bar of a.colorbars) {
          requireThat(record(bar) && bar.peer_axes === i && ["horizontal", "vertical"].includes(bar.orientation), path + ".colorbars", "invalid_index");
          numbers(bar.position, 4, path + ".colorbars.position");
          requireThat(bar.position[2] > 0 && bar.position[3] > 0 && ["normal", "reverse"].includes(bar.direction), path + ".colorbars.position");
          range(bar.limits, path + ".colorbars.limits");
          numbers(bar.ticks, undefined, path + ".colorbars.ticks");
          budget(bar.ticks.length, "ticks", path + ".colorbars.ticks");
          labels(bar.tick_labels, path + ".colorbars.tick_labels");
          text(bar.label, path + ".colorbars.label");
          colormap(bar.colormap, path + ".colorbars.colormap");
        }
      }
      seriesCount += a.series.length;
      budget(seriesCount, "series", path);
      let axisReduced = false;
      for (let j = 0; j < a.series.length; j++) {
        const s = a.series[j], p = path + ".series[" + j + "]";
        requireThat(record(s), p);
        if (payload.version === 3) requireThat(s.id === j, p + ".id", "invalid_index");
        if (s.kind === "surface") {
          requireThat(payload.version === 3 && dimension === 3, p);
          surface(s, p);
          triangles += s.triangle_count;
          vertices += s.rendered_points;
        } else if (s.kind === "image") {
          requireThat(payload.version === 3 && dimension === 2 && a.xscale === "linear" && a.yscale === "linear", p + ".kind", "unsupported_object", { type: "image" });
          requireThat(payload.limits.image_pixels === OPTIONAL_LIMITS.image_pixels, "limits.image_pixels");
          image(s, p);
          pixels += s.pixel_count;
        } else if (s.kind === "patch2d" || s.kind === "text") {
          // Filled shapes and free text exist only in 2D axes of a v3 figure.
          requireThat(payload.version === 3 && dimension === 2, p + ".kind", "unsupported_object", { type: s.kind });
          if (s.kind === "patch2d") {
            requireThat(a.xscale === "linear" && a.yscale === "linear", p, "unsupported_patch", { property: "scale" });
            patch2d(s, p);
            vertices += s.rendered_points;
          } else textRecord(s, p);
        } else {
          samples(s, payload.version, p);
          requireThat(!own(s, "z") || dimension === 3, p + ".z");
          vertices += s.x.length;
        }
        requireThat(STYLES.includes(s.line_style), p + ".line_style");
        text(s.display_name, p + ".display_name");
        axisReduced ||= s.decimated;
      }
      requireThat(a.decimated === axisReduced, path + ".decimated");
      reduced ||= axisReduced;
    }
    budget(pixels, "image_pixels", "pixel_count");
    requireThat((own(payload, "pixel_count") ? payload.pixel_count : 0) === pixels, "pixel_count");
    budget(vertices, "vertices", "vertex_count");
    budget(triangles, "triangles", "triangle_count");
    requireThat(payload.decimated === reduced, "decimated");
    if (payload.version === 3) requireThat(payload.vertex_count === vertices && payload.triangle_count === triangles, "counts");
    return { ok: true, supported: true, reason_code: "", reason_args: {}, data: payload, version: payload.version, vertex_count: vertices, triangle_count: triangles };
  } catch (e) {
    return { ok: false, supported: false, reason_code: e.reason_code || "invalid_data", reason_args: e.reason_args || {}, path: e.path || "figure" };
  }
}
module.exports = { validate, decodeArray, LIMITS, OPTIONAL_LIMITS, REASON_CODES };
