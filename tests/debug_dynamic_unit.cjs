const assert = require('assert');
const fs = require('fs');
const vm = require('vm');
const nodes = new Map();
function node() {
  return {children: [], style: {}, dataset: {}, append(...items) {this.children.push(...items);},
    replaceChildren() {this.children = [];}, setAttribute() {}};
}
const shared = {engine: {}, breakpoints: new Map(), uiGeneration: 1, lastEpoch: null,
  publishRenders: new Map(), busy: true};
const opened = [], toasts = [];
let state;
const registry = {
  $: selector => {if (!nodes.has(selector)) nodes.set(selector, node()); return nodes.get(selector);},
  el: (tag, cls, text) => Object.assign(node(), {textContent: text}),
  api: async () => state, setStatus() {}, refreshDebugEditor() {}, renderVariables() {},
  openDebugLocation: async (...args) => opened.push(args), toast: text => toasts.push(text)
};
const context = vm.createContext({shared, registry, t: text => text, onLanguageChange() {}});
for (const file of ['debugger', 'poll']) {
  let source = fs.readFileSync(`frontend/${file}.js`, 'utf8')
    .replace(/^import .*;\n/gm, '').replace(/^export .*;\n/gm, '');
  vm.runInContext(source, context);
}
registry.openDebugLocation = async (...args) => opened.push(args);
(async () => {
  state = {status: 'paused', job: 'dynamic', epoch: 1, variables: [],
    debug: {ready: true, serial: 1, file: '', line: 0, stack: []}};
  await registry.poll();
  assert.equal(nodes.get('#debugger-state').textContent, 'Paused — no source location');
  assert.equal(nodes.get('#debugger-panel').hidden, false);
  assert.equal(opened.length, 0);
  assert.equal(toasts.length, 0);
  state.debug = {ready: true, serial: 2, file: '/work/example.m', line: 3, stack: []};
  await registry.poll();
  assert.equal(nodes.get('#debugger-state').textContent, 'Paused');
  assert.deepEqual(opened, [['/work/example.m', 3]]);
  console.log('Dynamic debugger UI unit checks passed');
})().catch(error => {console.error(error); process.exitCode = 1;});
