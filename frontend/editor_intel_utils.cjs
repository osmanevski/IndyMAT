const editorCommandUtils = require("./editor_command_utils.cjs");

const identifierPattern = /[A-Za-z_][A-Za-z0-9_]*/g;
const functionPattern = /^\s*function(?:\s+(?:\[[^\]]*\]|[A-Za-z_]\w*)\s*=\s*|\s+)([A-Za-z_]\w*)\s*(?:\(|$)/i;
const functionDeclarationPattern = /^\s*function\s+(?:(\[[^\]]*\]|[A-Za-z_]\w*)\s*=\s*)?([A-Za-z_]\w*)\s*(?:\(([^)]*)\))?/i;

function parseEditorSymbols(source, translate = (source, params = {}) => source.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (placeholder, name) => Object.hasOwn(params, name) ? String(params[name]) : placeholder)) {
  const lines = editorCommandUtils.scanEditorSource(source);
  const sections = [];
  const functions = [];
  for (const line of lines) {
    if (line.section) sections.push({ kind: "section", name: line.text.replace(/^\s*%%\s*/, "").trim() || translate("Untitled section"), line: line.number, from: line.from });
    const match = line.code.match(functionPattern);
    if (match) functions.push({ kind: "function", name: match[1], line: line.number, from: line.from + match.index });
  }
  return { sections, functions };
}

function identifierAt(source, position) {
  position = Math.max(0, Math.min(source.length, Number(position) || 0));
  let start = position;
  let end = position;
  if (start === source.length || !/[A-Za-z0-9_]/.test(source[start] || "")) start--;
  if (start < 0 || !/[A-Za-z0-9_]/.test(source[start])) return "";
  while (start > 0 && /[A-Za-z0-9_]/.test(source[start - 1])) start--;
  end = Math.max(position, start);
  while (end < source.length && /[A-Za-z0-9_]/.test(source[end])) end++;
  const name = source.slice(start, end);
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ? name : "";
}

function occurrencesInSource(source, name) {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) return [];
  const found = [];
  for (const line of editorCommandUtils.scanEditorSource(source)) {
    identifierPattern.lastIndex = 0;
    for (let match; (match = identifierPattern.exec(line.code)); ) {
      if (match[0] === name) found.push({ name, line: line.number, column: match.index + 1, from: line.from + match.index });
    }
  }
  return found;
}

function variableDefinitionInSource(source, name) {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) return null;
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const assignment = new RegExp(`(?:^|[;,])\\s*(?:\\[[^\\]]*\\b${escaped}\\b[^\\]]*\\]|${escaped}(?:\\s*\\([^)]*\\))?)\\s*(?:=($|[^=])|[+\\-*/]=)`);
  const loop = new RegExp(`^\\s*(?:for|parfor)\\s+${escaped}\\s*=`);
  const declaration = new RegExp(`^\\s*(?:global|persistent|catch)\\s+[^%#]*\\b${escaped}\\b`);
  for (const line of editorCommandUtils.scanEditorSource(source)) {
    const functionDeclaration = functionDeclarationPattern.exec(line.code);
    const declared = functionDeclaration ? [...functionDeclaration[1]?.matchAll(identifierPattern) || [], ...functionDeclaration[3]?.matchAll(identifierPattern) || []].some((match) => match[0] === name) : false;
    const match = declared ? functionDeclaration : assignment.exec(line.code) || loop.exec(line.code) || declaration.exec(line.code);
    if (match) {
      const column = line.code.indexOf(name, match.index);
      return { kind: "variable", name, line: line.number, column: column + 1, from: line.from + column };
    }
  }
  return null;
}

module.exports = { identifierAt, occurrencesInSource, parseEditorSymbols, variableDefinitionInSource };
