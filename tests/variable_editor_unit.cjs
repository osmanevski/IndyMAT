const assert = require("assert");
const { typedCell, pasteGrid, selectionRect } = require("../frontend/variable_editor_utils.cjs");

assert.deepEqual(typedCell("logical", "false"), { type: "logical", value: false });
assert.deepEqual(typedCell("int64", "-9223372036854775808"), { type: "integer", value: "-9223372036854775808" });
assert.deepEqual(typedCell("double", "NaN"), { type: "special", value: "NaN" });
assert.deepEqual(typedCell("char", "ğİş🙂"), { type: "text", value: "ğİş🙂" });
assert.deepEqual(typedCell("char", ""), { type: "text", value: "" });
assert.throws(() => typedCell("char", "\uD800"), /Unicode/);
assert.throws(() => typedCell("char", "ğ".repeat(5001)), /10.000/);
for (const kind of ['double', 'single']) {
  assert.deepEqual(JSON.parse(JSON.stringify(typedCell(kind, '-0'))), {type:'special',value:'-0'});
  assert.deepEqual(typedCell(kind, '-0.0e5'), {type:'special',value:'-0'});
  assert.deepEqual(typedCell(kind, '0'), {type:'number',value:0});
}
assert.throws(() => typedCell("int8", "1.5"), /integer/);
assert.throws(() => typedCell("logical", "maybe", source => source === "Logical values must be true, false, 1, or 0." ? "Mantıksal değer true, false, 1 veya 0 olmalı." : source), /Mantıksal değer/);
assert.deepEqual(pasteGrid("1\t2\r\n3\t4\r\n"), [["1","2"],["3","4"]]);
assert.throws(() => pasteGrid("1\t2\n3"), /rectangular/);
assert.deepEqual(selectionRect({row:3,column:1},{row:1,column:4}), {top:1,bottom:3,left:1,right:4});
console.log("VARIABLE EDITOR UNIT PASS: typed cells, rectangular paste and selection bounds.");

// Run the actual editor handlers against a minimal event DOM, without a browser.
const fs = require('node:fs');
const vm = require('node:vm');
class Node {
  constructor(tag) {
    this.tagName = tag;
    this.children = [];
    this.dataset = {};
    this.listeners = {};
    this.open = false;
  }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
  set textContent(value) { this.text = value; this.children = []; }
  get textContent() { return this.text; }
  setAttribute() {}
  addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
  emit(name, event = {}) { for (const handler of this.listeners[name] || []) handler(event); }
  focus() { document.activeElement = this; }
  select() {}
  close() { this.open = false; this.emit('close'); }
  deferClose() { this.open = false; return () => this.emit('close'); }
}
const document = { activeElement: null };
const modal = new Node('dialog');
const closeButton = new Node('button');
const body = new Node('div');
const shared = {engine:{status:'idle',epoch:1},variables:[{name:'a',class:'int8'}],busy:false};
const calls = [];
const errors = [];
let tasks = [];
let rejected = false;
let modalPaints = 0;
const registry = {
  $: selector => ({'#modal':modal,'#modal-close':closeButton,'#modal-body':body}[selector]),
  el: (tag, cls, text) => {
    let node = new Node(tag);
    node.className = cls;
    node.textContent = text;
    return node;
  },
  safe: fn => {
    const task = Promise.resolve().then(fn).catch(error => errors.push(error.message));
    tasks.push(task);
    return task;
  },
  api: async (route, payload) => {
    if (rejected) { rejected = false; throw new Error('Sunucu reddetti'); }
    calls.push({route,payload,job:'job-' + calls.length});
    return {job:calls.at(-1).job};
  },
  requireIdle() {},
  setStatus() {},
  toast() {},
  modal(title, node) { modal.open = true; modal.title = title; body.replaceChildren(node); modalPaints++; }
};
let source = fs.readFileSync('frontend/variable_editor.js','utf8').replace(/^import .*;\n/gm,'');
source += '\nregistry.testEdit = beginCellEdit; registry.testTab = () => tabs.get("a"); registry.testRead = () => readTab(tabs.get("a"));';
vm.runInNewContext(source, {shared,registry,document,TextEncoder,variableUtils:require('../frontend/variable_editor_utils.cjs'),t:(source,params)=>require("../frontend/i18n_utils.cjs").translate(source,params),onLanguageChange:()=>()=>{}});
const workspaceSource = fs.readFileSync('frontend/workspace.js','utf8').replace(/^import .*;\n/gm,'');
vm.runInNewContext(workspaceSource + '\nregistry.testWorkspaceKeydown = workspaceKeydown;', {shared,registry,document,t:(source,params)=>require("../frontend/i18n_utils.cjs").translate(source,params),onLanguageChange:()=>()=>{}});
const key = (input, key) => input.onkeydown({key,stopPropagation(){},preventDefault(){}});
const drain = async () => {
  while (tasks.length) { const current = tasks; tasks = []; await Promise.all(current); }
};
const completeRead = async (call = calls.at(-1)) => {
  shared.busy = false;
  await registry.variableJobCompleted({job:call.job,kind:'variable-read',variable_action:{name:'a',class:'int8',root_class:'int8',size:[1,1],root_size:[1,1],real:true,editable:true,kind:'matrix',row:1,column:1,row_count:1,column_count:1,row_total:1,column_total:1,slices:[],rows:[['1']]}});
};
const open = async () => { shared.busy = false; await registry.openVariable('a'); await completeRead(); };
const edit = async () => {
  const cell = new Node('td');
  registry.testEdit(registry.testTab(),cell,0,0,'1');
  await drain();
  return {cell,input:cell.children[0]};
};
(async () => {
  // First editor open after a legacy dialog: even its queued close is unrelated.
  registry.showDetail({name:'legacy',class:'function_handle',size:[1,1],text:'@sin'});
  const legacyClose = modal.deferClose();
  await registry.inspect('a');
  legacyClose();
  await completeRead();
  assert.equal(modal.open,true,'first editor inspection must survive a queued legacy close');
  modal.close();

  // Match ui_workspace: inspect/close four times, then focus + Enter.
  // Native dialog.close() clears open immediately, but dispatches close later.
  let oldClose;
  for (const column of [0,1,2,3]) {
    await registry.inspect('a');
    await completeRead();
    assert.equal(modal.open,true, 'workspace double click column ' + column);
    oldClose = modal.deferClose();
    if (column < 3) oldClose();
  }
  registry.testWorkspaceKeydown({key:'Enter',preventDefault(){}},shared.variables[0],shared.variables);
  await drain();
  oldClose();
  await completeRead();
  assert.equal(modal.open,true,'queued previous close must not invalidate a new workspace Enter inspection');
  assert.equal(modal.title,'a — Variable View');

  // The opposite ordering: the new read paints before the old close arrives.
  oldClose = modal.deferClose();
  await registry.inspect('a');
  await completeRead();
  oldClose();
  let paints = modalPaints;
  await registry.testRead();
  await completeRead();
  assert.equal(modalPaints,paints + 1,'old close must not invalidate an already rendered new session');

  // Paused workspace inspection bypasses the variable editor; its structured
  // result uses showDetail (as the merged poll/debug-inspection path does).
  modal.close();
  shared.engine.status = 'paused';
  const debugCalls = [];
  registry.debugCommand = async (command, name) => debugCalls.push({command,name});
  const beforePaused = calls.length;
  await registry.inspect('middle_local');
  assert.equal(debugCalls.at(-1).name,'middle_local');
  assert.equal(calls.length,beforePaused,'paused inspection must not start a variable job');
  registry.showDetail({name:'middle_local',class:'double',size:[1,1],rows:[['14']]});
  assert.equal(modal.open,true,'closed editor session must not gate paused-frame details');
  assert.equal(modal.title,'middle_local — Variable View');
  oldClose = modal.deferClose();
  shared.engine.status = 'idle';
  await registry.inspect('a');
  oldClose();
  await completeRead();
  assert.equal(modal.open,true,'inspection after paused detail close must still open');

  await open();
  let {cell,input} = await edit();
  input.value = 'x';
  key(input,'Enter');
  await drain();
  assert(errors.at(-1).includes('integer'));
  assert.equal(cell.children[0],input,'invalid edit must stay correctable');
  input.value = '2';
  key(input,'Enter');
  await drain();
  assert.equal(calls.at(-1).payload.values[0].value,'2');

  await open();
  ({cell,input} = await edit());
  input.value = 'x';
  key(input,'Enter');
  await drain();
  key(input,'Escape');
  await drain();
  assert.equal(cell.textContent,'1','invalid edit must remain cancellable');
  assert.equal(modal.open,true,'Escape in inline edit must not close modal');

  ({cell,input} = await edit());
  input.value = '2';
  rejected = true;
  key(input,'Enter');
  await drain();
  assert.equal(errors.at(-1),'Sunucu reddetti');
  assert.equal(cell.children[0],input);
  input.value = '3';
  key(input,'Enter');
  await drain();
  assert.equal(calls.at(-1).payload.values[0].value,'3');

  await open();
  ({cell,input} = await edit());
  input.value = '4';
  let before = calls.length;
  closeButton.emit('pointerdown');
  input.onblur();
  modal.close();
  await drain();
  assert.equal(calls.length,before,'closing a dirty inline editor must not submit on blur');
  assert.equal(modal.open,false);

  await open();
  ({cell,input} = await edit());
  input.value = '5';
  key(input,'Enter');
  await drain();
  let write = calls.at(-1);
  before = calls.length;
  const deferredWriteClose = modal.deferClose();
  shared.busy = false;
  await registry.variableJobCompleted({job:write.job,kind:'variable-write'});
  assert.equal(calls.length,before,'closed write completion must not trigger a refresh');
  assert.equal(modal.open,false);
  deferredWriteClose();

  await open();
  await registry.testRead();
  let read = calls.at(-1);
  before = modalPaints;
  const deferredReadClose = modal.deferClose();
  await completeRead(read);
  assert.equal(modalPaints,before,'closed read completion must not reopen the dialog');
  deferredReadClose();

  await open();
  ({cell,input} = await edit());
  input.value = '9';
  key(input,'Enter');
  await drain();
  write = calls.at(-1);
  shared.busy = false;
  await registry.variableJobCompleted({job:write.job,kind:'variable-write',error:'Octave refused'});
  assert.equal(calls.at(-1).payload.action,'read','Octave rejection must refresh the failed editor');
  await completeRead();
  assert.equal(modal.open,true);
  console.log('VARIABLE EDITOR LIFECYCLE PASS: workspace reopen/Enter, queued close orderings, paused/legacy detail handoff, retryable edits and closed-dialog late-job protection.');
})().catch(error => { console.error(error); process.exitCode = 1; });
