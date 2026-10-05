const assert = require("node:assert/strict");
const { scanEditorSource, sectionRange, smartIndent, toggleComment } = require("../frontend/editor_command_utils.cjs");

function apply(source, edit) {
  return source.slice(0, edit.from) + edit.text + source.slice(edit.to);
}

const commentFixture = "  alpha = 1;\n    beta = 2;\n";
const commented = apply(commentFixture, toggleComment(commentFixture, 0, commentFixture.length));
assert.equal(commented, "  % alpha = 1;\n    % beta = 2;\n");
assert.equal(apply(commented, toggleComment(commented, 0, commented.length)), commentFixture);

// Finding 12: preserve both marker percents, alone and in mixed selections.
for (const original of ["%% Section", "  %% Section", "%% First\n%% Second", "%% Section\nx = 1;\n% existing comment"]) {
  const outer = toggleComment(original, 0, original.length);
  const restored = toggleComment(outer.text, 0, outer.text.length);
  assert.equal(outer.text.includes("% %%"), true);
  assert.equal(restored.text, original, "section comment toggles must round-trip");
}

const nestedFixture = [
  "function y = nested(x)",
  "if x",
  "disp('end; % if stays in the string');",
  "% switch and end stay in the comment",
  "switch x",
  "case 1",
  "try",
  "y = 1;",
  "catch",
  "y = 2;",
  "end_try_catch",
  "otherwise",
  "y = 3;",
  "endswitch",
  "endif",
  "endfunction"
].join("\n");
const expectedNested = [
  "function y = nested(x)",
  "    if x",
  "        disp('end; % if stays in the string');",
  "        % switch and end stay in the comment",
  "        switch x",
  "            case 1",
  "                try",
  "                    y = 1;",
  "                catch",
  "                    y = 2;",
  "                end_try_catch",
  "            otherwise",
  "                y = 3;",
  "        endswitch",
  "    endif",
  "endfunction"
].join("\n");
assert.equal(apply(nestedFixture, smartIndent(nestedFixture, 0, nestedFixture.length)), expectedNested);

const contextual = "if ready\nvalue = 1;\nend\nafter = 2;";
const contextualStart = contextual.indexOf("value");
const contextualEnd = contextual.indexOf("\nend");
assert.equal(apply(contextual, smartIndent(contextual, contextualStart, contextualEnd)), "if ready\n    value = 1;\nend\nafter = 2;");

const lexicalFixture = "if ready, value = 'end; if'; end\nnext = 1;\n%{\nif ignored\nend\n%}\nlast = 2;";
assert.equal(apply(lexicalFixture, smartIndent(lexicalFixture, 0, lexicalFixture.length)), lexicalFixture);

const sections = "setup = 1;\n%% Bir\na = 1;\n%% İki\nb = 2;\n";
assert.deepEqual(sectionRange(sections, sections.indexOf("a =")), { from: 11, to: 24, startLine: 2, endLine: 3, nextFrom: 25 });
assert.deepEqual(sectionRange(sections, sections.indexOf("b ="), true), { from: 25, to: sections.length, startLine: 4, endLine: 6, nextFrom: null });

// Finding 6: the reviewer example has active x after %}; the second fixture
// places x inside the comment, where slicing at the fake header exposed it.
for (const source of ["%{\n%% fake\n%}\nx = 17;\n%% real", "%{\n%% fake\nx = 17;\n%}\n%% real"]) {
  const range = sectionRange(source, source.indexOf("x ="));
  assert.equal(range.from, 0);
  assert.equal(range.startLine, 1);
  assert.equal(range.nextFrom, source.indexOf("%% real"));
  assert.equal(source.slice(range.from, range.to).startsWith("%{"), true);
  assert.equal(sectionRange(source, source.indexOf("x ="), true).from, 0);
  assert.deepEqual(scanEditorSource(source).filter((line) => line.section).map((line) => line.text), ["%% real"]);
}
const nestedComments = "%% first\n%{\n%{\n%% nested\n%}\n%% still hidden\n%}\n#{\n%% Octave hidden\n#}\n%% next";
assert.deepEqual(scanEditorSource(nestedComments).filter((line) => line.section).map((line) => line.text), ["%% first", "%% next"]);
assert.equal(sectionRange(nestedComments, nestedComments.indexOf("nested")).nextFrom, nestedComments.indexOf("%% next"));
assert.equal(scanEditorSource("%{\n%% hidden forever").some((line) => line.section), false);
assert.deepEqual(scanEditorSource("x = '%{ %% not a header';\ny = A';\n%% active").filter((line) => line.section).map((line) => line.text), ["%% active"]);
assert.deepEqual(scanEditorSource('x = "continued\\\n%% string text\\\n";\n%% active').filter((line) => line.section).map((line) => line.text), ["%% active"]);
assert.equal(scanEditorSource("%{\nif false\nend\n%}")[1].code, "", "commented blocks must not offer code folds");

console.log("EDITOR COMMAND UNIT PASS: section ranges, comment round trip, nested/contextual smart indent, strings and comments.");

// Exercise the installed CodeMirror dispatcher, including w3c-keyname's
// platform-specific processing, without starting a browser or an Octave session.
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { platform: "MacIntel", userAgent: "", vendor: "" }
});
const { EditorState } = require("@codemirror/state");
const { keymap, runScopeHandlers } = require("@codemirror/view");
const { allowGlobalEditorShortcut, editorCommandBindings } = require("../frontend/editor_shortcuts.cjs");
const invoked = [];
const registry = { safe: (fn) => fn() };
for (const command of ["runSelection", "runSection", "runAndAdvance", "runToEnd", "goToLine", "toggleEditorComment", "smartIndentSelection", "foldAllEditor", "unfoldAllEditor"]) {
  registry[command] = () => invoked.push(command);
}
const shortcutView = { state: EditorState.create({ extensions: [keymap.of(editorCommandBindings(registry))] }) };
function keyboardEvent(key, keyCode, modifiers = {}) {
  return { key, keyCode, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers };
}
function expectCommand(label, event, command) {
  invoked.length = 0;
  assert.equal(runScopeHandlers(shortcutView, event, "editor"), true, label);
  assert.deepEqual(invoked, [command], label + " must invoke exactly one command");
}
for (const layout of ["US", "Turkish Q"]) {
  expectCommand(layout + " Run Section", keyboardEvent("Enter", 13, { metaKey: true }), "runSection");
  expectCommand(layout + " Run and Advance", keyboardEvent("Enter", 13, { metaKey: true, shiftKey: true }), "runAndAdvance");
  expectCommand(layout + " Run to End", keyboardEvent("Enter", 13, { metaKey: true, shiftKey: true, altKey: true }), "runToEnd");
  expectCommand(layout + " Run Selection", keyboardEvent("F9", 120), "runSelection");
  expectCommand(layout + " Go to Line", keyboardEvent("g", 71, { ctrlKey: true }), "goToLine");
  expectCommand(layout + " Smart Indent", keyboardEvent("i", layout === "US" ? 73 : 222, { ctrlKey: true }), "smartIndentSelection");
  expectCommand(layout + " Fold All", keyboardEvent(",", 188, { ctrlKey: true }), "foldAllEditor");
  expectCommand(layout + " Unfold All", keyboardEvent(".", 190, { ctrlKey: true }), "unfoldAllEditor");
  expectCommand(layout + " Comment alternative", keyboardEvent("F9", 120, { shiftKey: true }), "toggleEditorComment");
}
expectCommand("Turkish Q dotless ı fallback", keyboardEvent("ı", 73, { ctrlKey: true }), "smartIndentSelection");
expectCommand("US Cmd+/", keyboardEvent("/", 191, { metaKey: true }), "toggleEditorComment");

// Preserve the observed failures as regression evidence for the chosen keys.
invoked.length = 0;
assert.equal(runScopeHandlers(shortcutView, keyboardEvent("/", 55, { metaKey: true, shiftKey: true }), "editor"), false, "macOS Turkish Cmd+Shift+7 is translated to US & by w3c-keyname");
assert.deepEqual(invoked, []);
const oldFoldView = { state: EditorState.create({ extensions: [keymap.of([
  { key: "Ctrl-,", run: () => {
    invoked.push("fold");
    return true;
  } },
  { key: "Shift-Ctrl-,", run: () => {
    invoked.push("unfold");
    return true;
  } }
])] }) };
runScopeHandlers(oldFoldView, keyboardEvent(",", 188, { ctrlKey: true, shiftKey: true }), "editor");
assert.deepEqual(invoked, ["fold"], "the old shifted-comma pair resolves to Fold All");
console.log("EDITOR KEYMAP PASS: actual CodeMirror dispatch, US/Turkish Q events, punctuation regressions, one command per shortcut.");

// Finding 11: the production window-handler guard keeps the command line
// exception, but blocks every modal and all other editable controls.
const commandTarget = {};
const editorTarget = { closest: (selector) => selector === "#editor" };
const toolbarTarget = { closest: () => null };
const documentFixture = (open) => ({
  querySelector: (selector) => selector === "dialog[open]" ? (open ? {} : null) : commandTarget
});
for (const event of [keyboardEvent("F5", 116), keyboardEvent("F9", 120), keyboardEvent("s", 83, { metaKey: true }), keyboardEvent("s", 83, { ctrlKey: true })]) {
  for (const target of [commandTarget, editorTarget, toolbarTarget]) {
    assert.equal(allowGlobalEditorShortcut({ ...event, target }, documentFixture(true)), false);
    assert.equal(allowGlobalEditorShortcut({ ...event, target }, documentFixture(false)), true);
  }
  for (const tag of ["input", "textarea", "select", "contenteditable"]) {
    const target = { closest: (selector) => selector.includes(tag), isContentEditable: tag === "contenteditable" };
    assert.equal(allowGlobalEditorShortcut({ ...event, target }, documentFixture(false)), false, tag);
  }
}
for (const modifier of ["shiftKey", "altKey", "ctrlKey", "metaKey"]) {
  assert.equal(allowGlobalEditorShortcut({ ...keyboardEvent("F9", 120, { [modifier]: true }), target: commandTarget }, documentFixture(false)), false);
}
console.log("EDITOR REVIEW REGRESSIONS PASS: lexical sections, section-comment round trip, global shortcut focus/modifier guards.");
