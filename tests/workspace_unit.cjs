// DOM event regression harness; numerical behavior is exercised by real Octave tests.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

class Element {
  constructor(tag) {
    this.tagName = tag;
    this.children = [];
    this.dataset = {};
    this.attributes = {};
    this.className = '';
    this.value = '';
    this.classList = {
      toggle: (name, on) => {
        const values = new Set(this.className.split(' ').filter(Boolean));
        if (on) values.add(name);
        else values.delete(name);
        this.className = [...values].join(' ');
      },
      contains: name => this.className.split(' ').includes(name)
    };
  }
  append(...nodes) {
    for (const node of nodes) {
      node.parent = this;
      this.children.push(node);
    }
  }
  replaceChildren(...nodes) {
    this.children = [];
    this.append(...nodes);
  }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  focus() {
    document.activeElement = this;
    this.onfocus?.();
  }
  select() {}
}

const ids = new Map();
for (const id of ['variables','variable-search','variable-count','workspace-empty','workspace-delete','workspace-edit-value','workspace-save-selection','workspace-save-all','workspace-load','clear-workspace']) ids.set('#' + id, new Element('div'));
const rows = () => ids.get('#variables').children;
const document = { activeElement: null, querySelectorAll: selector => selector === '#variables tr' ? rows() : [] };
const shared = {
  variables: [{name:'a',size:'1x1',class:'double',preview:'1',bytes:8},{name:'b',size:'1x1',class:'double',preview:'2',bytes:8}],
  engine: {status:'idle',epoch:1}
};
const calls = [];
const messages = [];
const handlers = new Map();
const pending = [];
const registry = {
  $: selector => {
    if (ids.has(selector)) return ids.get(selector);
    const match = selector.match(/^#variables tr\[data-name="([^"]+)"\](.*)$/);
    if (match) {
      const row = rows().find(item => item.dataset.name === match[1]);
      return match[2] ? row?.children[1] : row;
    }
    throw Error(selector);
  },
  el: (tag, cls, text) => {
    const node = new Element(tag);
    node.className = cls || '';
    node.textContent = text;
    return node;
  },
  on: (selector, handler) => handlers.set(selector, handler),
  safe: fn => {
    const result = Promise.resolve().then(fn);
    pending.push(result);
    return result;
  },
  requireIdle: () => {},
  api: async (path, payload) => {
    calls.push({path,payload});
    return {job:'job-' + calls.length};
  },
  setStatus: () => {},
  toast: message => messages.push(message)
};
let source = fs.readFileSync('frontend/workspace.js','utf8').replace(/^import .*;\n/gm,'');
source += '\nregistry.testScalar = parseWorkspaceScalar;';
vm.runInNewContext(source,{registry,shared,document,CSS:{escape:s=>s},EditorView:{},console,confirm:()=>true,t:(source,params)=>require("../frontend/i18n_utils.cjs").translate(source,params),onLanguageChange:()=>()=>{}});
const click = (node, extra = {}) => {
  const event = {...extra,stopPropagation(){this.stopped = true;}};
  for (let current = node; current; current = current.parent) {
    current.onclick?.(event);
    if (event.stopped) break;
  }
};
const key = (node, value) => node.onkeydown({key:value,currentTarget:node,preventDefault(){},stopPropagation(){}});

(async () => {
  registry.setupVariableSearch();
  registry.renderVariables();
  const original = rows()[0];
  click(original);
  assert.equal(rows()[0],original);
  key(original,'F2');
  let input = original.children[0].children[0];
  input.value = 'draft_name';
  input.oninput();
  // A second browser's completed job refreshes variables while this edit is open.
  shared.variables[1] = {...shared.variables[1],preview:'27'};
  registry.renderVariables();
  registry.renderVariables();
  assert.equal(rows()[0],original);
  assert.equal(original.children[0].children[0],input);
  assert.equal(document.activeElement,input);
  assert.equal(input.value,'draft_name');
  assert.equal(calls.length,0,'polling must not submit a draft');
  key(input,'Escape');
  assert.equal(rows()[1].children[1].textContent,'27','deferred refresh must reconcile after closing edit');

  click(rows()[0]);
  handlers.get('#workspace-edit-value')();
  input = rows()[0].children[1].children[0];
  input.value = '7';
  input.oninput();
  shared.variables[1] = {...shared.variables[1],preview:'28'};
  registry.renderVariables();
  assert.equal(rows()[0].children[1].children[0],input);
  assert.equal(document.activeElement,input);
  assert.equal(input.value,'7');
  const staleBlur = input.onblur;
  shared.variables[0] = {...shared.variables[0],class:'int8',preview:'3'};
  registry.renderVariables();
  staleBlur();
  await Promise.resolve();
  assert.equal(calls.length,0,'invalidated editor must never commit on blur');
  assert(messages.at(-1).includes('canceled'));

  click(rows()[0]);
  key(rows()[0],'F2');
  input = rows()[0].children[0].children[0];
  shared.engine.epoch++;
  registry.renderVariables();
  key(input,'Enter');
  assert.equal(calls.length,0,'reset must invalidate draft');

  shared.variables[0].global = true;
  registry.renderVariables();
  key(rows()[0],'F2');
  assert(messages.at(-1).includes('Global'));
  assert.equal(rows()[0].children[0].children[0].tagName,'span');

  for (const special of ['NaN','Inf','-Inf']) {
    const value = registry.testScalar({class:'single'},special);
    assert.equal(value.special,special);
  }
  assert.throws(() => registry.testScalar({class:'single'},'1e300'));
  assert.throws(() => registry.testScalar({class:'double'},''));
  assert.throws(() => registry.testScalar({class:'char'},'ğ'));
  assert.equal(registry.testScalar({class:'char'},'x'),'x');
  console.log('WORKSPACE UNIT PASS: polling preserves drafts/focus; explicit conflict/reset cancellation; global guard; typed special values, single range, ASCII char policy.');
})().catch(error => {console.error(error);process.exitCode = 1;});
