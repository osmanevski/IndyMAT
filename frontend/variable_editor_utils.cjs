const integerClasses = new Set(["int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64"]);

function identity(source, params = {}) {
  return source.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (match, name) => Object.hasOwn(params, name) ? String(params[name]) : match);
}

function typedCell(className, text, translate = identity) {
  if (className === "logical") {
    if (/^(true|1)$/i.test(text.trim())) return { type: "logical", value: true };
    if (/^(false|0)$/i.test(text.trim())) return { type: "logical", value: false };
    throw new Error(translate("Logical values must be true, false, 1, or 0."));
  }
  if (className === "char") {
    if (/[\uD800-\uDFFF]/u.test(text)) throw new Error(translate("Text must be valid Unicode."));
    if (new TextEncoder().encode(text).length > 10000) throw new Error(translate("Text editing is limited to 10,000 UTF-8 bytes."));
    return { type: "text", value: text };
  }
  if (integerClasses.has(className)) {
    let value = text.trim();
    if (value.length > 21 || !/^-?(?:0|[1-9][0-9]*)$/.test(value)) throw new Error(translate("Enter a bounded integer without decimals."));
    return { type: "integer", value };
  }
  if (["double", "single"].includes(className)) {
    let value = text.trim();
    if (["NaN", "Inf", "-Inf"].includes(value)) return { type: "special", value };
    if (!value) throw new Error(translate("Enter a number."));
    let number = Number(value);
    if (!Number.isFinite(number)) throw new Error(translate("Enter a finite number or NaN, Inf, or -Inf."));
    if (Object.is(number, -0)) return { type: "special", value: "-0" };
    return { type: "number", value: number };
  }
  throw new Error(translate("This array class cannot be edited."));
}

function pasteGrid(text, translate = identity) {
  let normalized = String(text).replace(/\r\n?/g, "\n");
  if (normalized.endsWith("\n")) normalized = normalized.slice(0, -1);
  let rows = normalized.split("\n").map((row) => row.split("\t"));
  let width = rows[0]?.length || 0;
  if (!width || rows.some((row) => row.length !== width)) throw new Error(translate("The pasted range must be rectangular."));
  return rows;
}

function selectionRect(anchor, focus) {
  if (!anchor || !focus) return null;
  return { top: Math.min(anchor.row, focus.row), bottom: Math.max(anchor.row, focus.row), left: Math.min(anchor.column, focus.column), right: Math.max(anchor.column, focus.column) };
}

module.exports = { integerClasses, typedCell, pasteGrid, selectionRect };
