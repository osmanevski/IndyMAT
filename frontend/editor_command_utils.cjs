const OPENERS = new Set(["if", "for", "parfor", "while", "switch", "try", "function", "do", "unwind_protect", "classdef", "methods", "properties", "events", "enumeration", "spmd"]);
const BRANCHES = new Set(["else", "elseif", "catch", "unwind_protect_cleanup"]);
const CASE_BRANCHES = new Set(["case", "otherwise"]);
const CLOSERS = new Set(["end", "endif", "endfor", "endparfor", "endwhile", "endswitch", "endfunction", "end_try_catch", "end_unwind_protect", "endclassdef", "endmethods", "endproperties", "endevents", "endenumeration", "endspmd", "until"]);

function clampPosition(source, value) {
  return Math.max(0, Math.min(source.length, Number(value) || 0));
}

function selectedLineSpan(source, from, to) {
  from = clampPosition(source, from);
  to = clampPosition(source, to);
  if (to < from) [from, to] = [to, from];
  let start = source.lastIndexOf("\n", from - 1) + 1;
  let lastSelected = to > from && source[to - 1] === "\n" ? to - 1 : to;
  let nextBreak = source.indexOf("\n", lastSelected);
  let end = nextBreak < 0 ? source.length : nextBreak;
  return { start, end };
}

function toggleComment(source, from, to) {
  let span = selectedLineSpan(source, from, to);
  let lines = source.slice(span.start, span.end).split("\n");
  // A section marker is source, not an outer line-comment prefix.
  let uncomment = lines.every((line) => /^[ \t]*%(?!%)/.test(line));
  let replacement = lines.map((line) => {
    if (uncomment) return line.replace(/^([ \t]*)% ?/, "$1");
    let indent = line.match(/^[ \t]*/)[0];
    return indent + "% " + line.slice(indent.length);
  }).join("\n");
  return { from: span.start, to: span.end, text: replacement, selectionFrom: span.start, selectionTo: span.start + replacement.length };
}

function startsString(line, index) {
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous--;
  return previous < 0 || /[=([{,;:+\-*/\\^~<>|&]/.test(line[previous]);
}

function statementTokens(line, lexicalState) {
  let trimmed = line.trimStart();
  if (lexicalState.blockComment) {
    if (/^%\}/.test(trimmed)) lexicalState.blockComment = false;
    return [];
  }
  if (/^%\{(?:\s|$)/.test(trimmed)) {
    lexicalState.blockComment = true;
    return [];
  }
  let segments = [""];
  let quote = "";
  let depth = 0;
  for (let i = 0; i < line.length; i++) {
    let character = line[i];
    if (quote) {
      segments[segments.length - 1] += " ";
      if (character === quote) {
        if (line[i + 1] === quote) {
          segments[segments.length - 1] += " ";
          i++;
        } else quote = "";
      } else if (character === "\\" && quote === '"' && i + 1 < line.length) {
        segments[segments.length - 1] += " ";
        i++;
      }
      continue;
    }
    if (character === "%") break;
    if (character === '"' || character === "'" && startsString(line, i)) {
      quote = character;
      segments[segments.length - 1] += " ";
      continue;
    }
    if ("([{\u007b".includes(character)) depth++;
    if (")]\u007d".includes(character)) depth = Math.max(0, depth - 1);
    if ((character === ";" || character === ",") && depth === 0) segments.push("");
    else segments[segments.length - 1] += character;
  }
  return segments.map((segment) => segment.match(/^\s*([A-Za-z_]\w*)\b/)?.[1].toLowerCase()).filter(Boolean);
}

function advanceStructure(state, tokens) {
  for (let token of tokens) {
    if (CASE_BRANCHES.has(token)) {
      if (state.stack.at(-1) === "case") state.stack.pop();
      state.stack.push("case");
    } else if (CLOSERS.has(token)) {
      if (state.stack.at(-1) === "case") state.stack.pop();
      state.stack.pop();
    } else if (OPENERS.has(token)) state.stack.push(token);
  }
}

function indentDepth(state, token) {
  let stack = [...state.stack];
  if (CASE_BRANCHES.has(token)) {
    if (stack.at(-1) === "case") stack.pop();
    return stack.length;
  }
  if (CLOSERS.has(token)) {
    if (stack.at(-1) === "case") stack.pop();
    stack.pop();
    return stack.length;
  }
  if (BRANCHES.has(token)) return Math.max(0, stack.length - 1);
  return stack.length;
}

function smartIndent(source, from, to, unit = "    ") {
  let span = selectedLineSpan(source, from, to);
  let before = source.slice(0, span.start).split("\n");
  if (before.at(-1) === "") before.pop();
  let structuralState = { blockComment: false, stack: [] };
  for (let line of before) advanceStructure(structuralState, statementTokens(line, structuralState));
  let lines = source.slice(span.start, span.end).split("\n");
  let formatted = [];
  for (let line of lines) {
    let tokens = statementTokens(line, structuralState);
    let first = tokens[0] || "";
    let lineDepth = indentDepth(structuralState, first);
    formatted.push(line.trim() ? unit.repeat(lineDepth) + line.replace(/^[ \t]*/, "") : "");
    advanceStructure(structuralState, tokens);
  }
  let replacement = formatted.join("\n");
  return { from: span.start, to: span.end, text: replacement, selectionFrom: span.start, selectionTo: span.start + replacement.length };
}

function sourceLines(source) {
  let result = [];
  let from = 0;
  for (let number = 1; ; number++) {
    let newline = source.indexOf("\n", from);
    let to = newline < 0 ? source.length : newline;
    let text = source.slice(from, to).replace(/\r$/, "");
    result.push({ number, from, to, text });
    if (newline < 0) return result;
    from = newline + 1;
  }
}

// One lexical pass for execution boundaries, section decoration and folding.
// Delimiters must occupy a whole line, as in Octave/MATLAB. Nested Octave
// comments and continued strings must never expose a marker inside them.
function scanEditorSource(source, profile = "native-octave") {
  let blocks = 0;
  let quote = "";
  return sourceLines(source).map((line) => {
    let section = false;
    let code = "";
    if (!quote && /^[ \t]*[%#]\{[ \t]*$/.test(line.text)) {
      blocks++;
      return { ...line, section, code };
    }
    if (blocks) {
      if (/^[ \t]*[%#]\}[ \t]*$/.test(line.text)) blocks--;
      return { ...line, section, code };
    }
    if (!quote) section = /^\s*%%/.test(line.text);
    for (let index = 0; index < line.text.length; index++) {
      const character = line.text[index];
      if (quote) {
        code += " ";
        if (character === "\\" && quote === '"' && profile !== "matlab") {
          if (index + 1 < line.text.length) {
            code += " ";
            index++;
          }
        } else if (character === quote) {
          if (line.text[index + 1] === quote) {
            code += " ";
            index++;
          } else quote = "";
        }
      } else if (character === "%" || character === "#") {
        break;
      } else if (line.text.slice(index, index + 3) === "...") {
        break;
      } else if (character === '"' || character === "'" && startsString(line.text, index)) {
        quote = character;
        code += "0";
      } else {
        code += character;
      }
    }
    return { ...line, section, code };
  });
}

function sectionRange(source, position, throughEnd = false, profile = "native-octave") {
  let lines = scanEditorSource(source, profile);
  position = clampPosition(source, position);
  let current = 0;
  while (current + 1 < lines.length && lines[current + 1].from <= position) current++;
  let start = 0;
  for (let index = current; index >= 0; index--) {
    if (lines[index].section) {
      start = index;
      break;
    }
  }
  let next = -1;
  for (let index = current + 1; index < lines.length; index++) {
    if (lines[index].section) {
      next = index;
      break;
    }
  }
  let end = throughEnd || next < 0 ? source.length : lines[next - 1].to;
  return { from: lines[start].from, to: end, startLine: lines[start].number, endLine: throughEnd || next < 0 ? lines.at(-1).number : lines[next - 1].number, nextFrom: next < 0 ? null : lines[next].from };
}

module.exports = { scanEditorSource, sectionRange, selectedLineSpan, smartIndent, toggleComment };
