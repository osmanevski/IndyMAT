const editorLexical = require("./editor_command_utils.cjs");

const OPENERS = new Set(["if", "for", "parfor", "while", "switch", "try", "function", "do", "unwind_protect", "classdef", "methods", "properties", "events", "enumeration", "spmd"]);
const CLOSERS = new Set(["end", "endif", "endfor", "endparfor", "endwhile", "endswitch", "endfunction", "end_try_catch", "end_unwind_protect", "endclassdef", "endmethods", "endproperties", "endevents", "endenumeration", "endspmd", "until"]);

function commandLexicalState(source) {
  const lines = editorLexical.scanEditorSource(source);
  let stack = [];
  let brackets = [];
  for (const line of lines) {
    let statement = "";
    const finishStatement = () => {
      const token = statement.match(/^\s*([A-Za-z_]\w*)\b/)?.[1].toLowerCase();
      if (CLOSERS.has(token)) stack.pop();
      else if (OPENERS.has(token)) stack.push(token);
      statement = "";
    };
    for (const character of line.code) {
      if ("([{".includes(character)) {
        if (!brackets.length) statement += " 0 ";
        brackets.push(character);
      }
      else if (")]}".includes(character)) {
        let expected = { ")": "(", "]": "[", "}": "{" }[character];
        if (brackets.at(-1) === expected) brackets.pop();
      } else if (!brackets.length) {
        if (character === ";" || character === ",") finishStatement();
        else statement += character;
      }
    }
    finishStatement();
  }
  return { blocks: stack.length, brackets: brackets.length, continuation: trailingContinuation(source) };
}

function trailingContinuation(source) {
  let blockDepth = 0;
  let quote = "";
  let lastCode = "";
  for (const rawLine of source.replace(/\r\n?/g, "\n").split("\n")) {
    let trimmed = rawLine.trim();
    if (!quote && /^%\{(?:\s|$)/.test(trimmed) || !quote && /^#\{(?:\s|$)/.test(trimmed)) {
      blockDepth++;
      continue;
    }
    if (blockDepth) {
      if (/^[%#]\}(?:\s|$)/.test(trimmed)) blockDepth--;
      continue;
    }
    let code = "";
    for (let index = 0; index < rawLine.length; index++) {
      let character = rawLine[index];
      if (quote) {
        if (character === "\\" && quote === '"' && index + 1 < rawLine.length) index++;
        else if (character === quote) {
          if (rawLine[index + 1] === quote) index++;
          else quote = "";
        }
        continue;
      }
      if (character === "%" || character === "#") break;
      if (character === '"' || character === "'" && startsString(rawLine, index)) quote = character;
      else code += character;
    }
    if (code.trim()) lastCode = code.trimEnd();
  }
  return lastCode.endsWith("...");
}

function startsString(line, index) {
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous--;
  return previous < 0 || /[=([{,;:+\-*/\\^~<>|&]/.test(line[previous]);
}

function commandIsIncomplete(source) {
  let state = commandLexicalState(source);
  return state.blocks > 0 || state.brackets > 0 || state.continuation;
}

function historyRecall(commands, draft, prefix) {
  return { draft, prefix, matches: commands.filter((command) => command.startsWith(prefix)), index: 0 };
}

function parseErrorLocations(text, limit = 64) {
  const locations = [];
  let offset = 0;
  // Scan lines and whitespace-delimited tokens once. Unanchored path regexes
  // retry at every byte of a long nonmatching token and are themselves quadratic.
  for (const line of text.split("\n")) {
    if (locations.length >= limit) break;
    let match = line.match(/near[ \t]+line[ \t]+(\d{1,8})(?:[ \t]*,?[ \t]*column[ \t]+\d{1,8})?[ \t]+(?:in|of)[ \t]+file[ \t]+(.+)$/i);
    if (match && match[2].length <= 4096) {
      const start = offset + match.index + match[0].lastIndexOf(match[2]);
      locations.push({ start, end: offset + line.length, path: match[2].trimEnd(), line: Number(match[1]) });
    } else {
      // The application formats caught frames as indented parent>child:line.
      const trimmed = line.trim();
      if (trimmed.length <= 4200) {
        match = trimmed.match(/^([A-Za-z_]\w*(?:>[A-Za-z_]\w*)*):(\d{1,8})$/);
        if (match && /^[ \t]/.test(line)) {
          const start = offset + line.indexOf(trimmed);
          locations.push({ start, end: start + trimmed.length, path: match[1], line: Number(match[2]) });
        } else {
          match = trimmed.match(/^(\S+)[ \t]+at[ \t]+line[ \t]+(\d{1,8})(?:[ \t]*,?[ \t]*column[ \t]+\d{1,8})?$/i);
          if (match) {
            const start = offset + line.indexOf(trimmed);
            locations.push({ start, end: start + trimmed.length, path: match[1], line: Number(match[2]) });
          }
        }
      }
      if (!match) {
        const tokens = /\S+/g;
        let token;
        while (locations.length < limit && (token = tokens.exec(line))) {
          if (token[0].length > 4200) continue;
          const location = token[0].match(/^([^:]+\.m):(\d{1,8})(?::\d{1,8})?$/);
          if (location) locations.push({ start: offset + token.index, end: offset + token.index + token[0].length, path: location[1], line: Number(location[2]) });
        }
      }
    }
    offset += line.length + 1;
  }
  return locations.filter((item) => item.line > 0 && item.path.length <= 4096);
}

function decorationSegments(length, matches, locations) {
  const segments = [];
  let position = 0, matchIndex = 0, locationIndex = 0;
  while (position < length) {
    while (matchIndex < matches.length && matches[matchIndex].end <= position) matchIndex++;
    while (locationIndex < locations.length && locations[locationIndex].end <= position) locationIndex++;
    const nextMatch = matches[matchIndex], nextLocation = locations[locationIndex];
    const match = nextMatch?.start <= position ? nextMatch : null;
    const location = nextLocation?.start <= position ? nextLocation : null;
    const end = Math.min(length, nextMatch ? match ? match.end : nextMatch.start : length, nextLocation ? location ? location.end : nextLocation.start : length);
    segments.push({ start: position, end, match, location });
    position = end;
  }
  return segments;
}

module.exports = { commandIsIncomplete, commandLexicalState, historyRecall, parseErrorLocations, trailingContinuation, decorationSegments };
