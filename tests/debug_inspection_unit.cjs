const assert = require('assert');
const fs = require('fs');
const vm = require('vm');

const calls = [];
const shared = {engine: {status: 'paused'}};
const registry = {
  debugCommand: async (command, code) => calls.push({command, code}),
  requireIdle: () => { throw new Error('idle path used'); },
  api: async () => { throw new Error('execute path used'); }
};
let source = fs.readFileSync('frontend/workspace.js', 'utf8').replace(/^import .*;\n/gm, '');
source += '\nglobalThis.inspectVariable = inspect;';
vm.runInNewContext(source, {registry, shared, document: {}, CSS: {}, EditorView: {}, confirm: () => true, console, t: (source, params) => require("../frontend/i18n_utils.cjs").translate(source, params), onLanguageChange: () => () => {}});

(async () => {
  await vm.runInNewContext("inspectVariable('middle_local')", {inspectVariable: registry.inspect});
  assert.deepStrictEqual(calls, [{command: 'inspect', code: 'middle_local'}]);
  console.log('DEBUG INSPECTION UNIT PASS: paused workspace inspection uses the structured debugger command.');
})().catch(error => {
  console.error(error);
  process.exit(1);
});
