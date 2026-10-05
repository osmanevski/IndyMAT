const assert=require('assert');
const fs=require('fs');
const path=require('path');
const {randomUUID}=require('crypto');

(async()=>{
 const launch=JSON.parse(fs.readFileSync('.matlab-free/launch.json','utf8'));
 const [base,token]=launch.url.split('#');
 const request=(endpoint,body)=>fetch(base+endpoint,{method:body===undefined?'GET':'POST',headers:{'X-MF-Language':'tr','X-MF-Token':token,...body===undefined?{}:{'Content-Type':'application/json'}},body:body===undefined?undefined:JSON.stringify(body)});
 const json=async response=>({status:response.status,body:await response.json()});
 const waitFor=async predicate=>{for(let index=0;index<300;index++){let state=await (await request('api/state')).json();if(predicate(state))return state;await new Promise(resolve=>setTimeout(resolve,50));}throw new Error('Workspace job timeout');};
 const waitIdle=()=>waitFor(state=>state.status==='idle'||state.status==='dead');
 const start=await (await request('api/state')).json();
 const epoch=start.epoch;
 const suffix=randomUUID().replaceAll('-','');
 const prefix='http_ws_'+suffix;
 const source=prefix+'_source',renamed=prefix+'_renamed',logical=prefix+'_logical',character=prefix+'_char',kept=prefix+'_kept';
 const mat=path.join(start.current,prefix+'.mat');
 const debugName=prefix+'_debug.m',debugPath=path.join(start.current,debugName);
 const alias=path.join(start.current,prefix+'_alias.mat');
 const job=async body=>{let response=await request('api/workspace',body);assert.equal(response.status,202,JSON.stringify(await response.json()));return waitIdle();};
 try{
  let response=await request('api/execute',{mode:'code',code:`${source}=11;${logical}=false;${character}='a';${kept}=777;`});assert.equal(response.status,202);await waitIdle();
  let state=await job({action:'rename',old_name:source,new_name:renamed});assert(!state.error,state.error);assert(state.variables.some(item=>item.name===renamed&&item.preview==='11'));assert(state.variables.some(item=>item.name===kept));
  state=await job({action:'assign-scalar',name:renamed,class:'double',value:12.5});assert(!state.error,state.error);assert.equal(state.variables.find(item=>item.name===renamed).preview,'12.5');
  state=await job({action:'assign-scalar',name:logical,class:'logical',value:true});assert(!state.error,state.error);state=await job({action:'assign-scalar',name:character,class:'char',value:'z'});assert(!state.error,state.error);
  for(const bad of [`${renamed}'`,`${renamed};${kept}=0`,`${renamed}\n${kept}=0`,`${renamed}()`,`__mf_${suffix}`]){let rejected=await json(await request('api/workspace',{action:'rename',old_name:bad,new_name:prefix+'_safe'}));assert.equal(rejected.status,400,bad);}
  response=await request('api/execute',{mode:'code',code:`assert(${kept}==777 && ${renamed}==12.5 && ${logical} && strcmp(${character},'z'));`});assert.equal(response.status,202);state=await waitIdle();assert(!state.error,state.error);
  state=await job({action:'save',path:mat,all:false,names:[renamed,logical,character],overwrite:false});assert(!state.error,state.error);assert(fs.existsSync(mat));assert.equal(state.epoch,epoch);
  let refused=await json(await request('api/workspace',{action:'save',path:mat,all:false,names:[renamed],overwrite:false}));assert.equal(refused.status,409);assert(refused.body.error.includes('zaten var'));const expectedHash=refused.body.error.match(/\[sha256:([a-f0-9]{64})\]/)[1];
  const approval=refused.body.error.match(/\[approval:([a-f0-9]{48})\]/)[1];
  refused=await json(await request('api/workspace',{action:'save',path:mat,all:false,names:[renamed],overwrite:true}));assert.equal(refused.status,400);assert(refused.body.error.includes('onay'));
  state=await job({action:'save',path:mat,all:false,names:[renamed,logical,character],overwrite:true,confirm_overwrite:true,expected_hash:expectedHash,approval});assert(!state.error,state.error);
  state=await job({action:'clear-names',names:[renamed,logical,character],confirm:true});assert(!state.error,state.error);assert(!state.variables.some(item=>item.name===renamed));assert(state.variables.some(item=>item.name===kept));
  response=await request('api/execute',{mode:'code',code:`${renamed}=999;`});assert.equal(response.status,202);await waitIdle();
  state=await job({action:'load-inspect',path:mat});assert(!state.error,state.error);assert.equal(state.kind,'workspace-load-inspect');assert(state.workspace_action.variables.some(item=>item.name===renamed));assert.deepEqual(state.workspace_action.replacements,[renamed]);
  refused=await json(await request('api/workspace',{action:'load',path:mat,hash:state.workspace_action.hash,replacements:[renamed]}));assert.equal(refused.status,400);assert(refused.body.error.includes('onay'));
  let inspect=state.workspace_action;
  state=await job({action:'load',path:mat,hash:inspect.hash,inspection:inspect.inspection,replacements:[],confirm:true});assert(state.error&&state.error.includes('farklılaştı'),state);assert.equal(state.variables.find(item=>item.name===renamed).preview,'999');
  state=await job({action:'load-inspect',path:mat});inspect=state.workspace_action;
  fs.writeFileSync(mat,'source replaced after approval');
  state=await job({action:'load',path:mat,hash:inspect.hash,inspection:inspect.inspection,replacements:inspect.replacements,confirm:true});assert(!state.error,state.error);assert.equal(state.variables.find(item=>item.name===renamed).preview,'12.5');assert(state.variables.some(item=>item.name===kept));
  refused=await json(await request('api/workspace',{action:'load',path:mat,hash:inspect.hash,inspection:inspect.inspection,replacements:inspect.replacements,confirm:true}));assert.equal(refused.status,400,'inspection capability must be one-shot');
  refused=await json(await request('api/workspace',{action:'assign-scalar',name:renamed,class:'single',value:1e300}));assert.equal(refused.status,400);
  state=await job({action:'assign-scalar',name:renamed,class:'int8',value:1});assert(state.error,'live class mismatch must fail');assert.equal(state.variables.find(item=>item.name===renamed).class,'double');
  for(const special of ['NaN','Inf','-Inf']){state=await job({action:'assign-scalar',name:renamed,class:'double',value:{special}});assert(!state.error,state.error);assert.equal(state.variables.find(item=>item.name===renamed).preview,special);}
  refused=await json(await request('api/workspace',{action:'load-inspect',path:'/etc/passwd.mat'}));assert.equal(refused.status,403);
  fs.symlinkSync(mat,alias);refused=await json(await request('api/workspace',{action:'load-inspect',path:alias}));assert.equal(refused.status,403);fs.unlinkSync(alias);
  fs.writeFileSync(debugPath,`${prefix}_debug_a=1;\n${prefix}_debug_b=2;\n`);
  response=await request('api/breakpoint',{path:debugPath,line:2,enabled:true});assert.equal(response.status,202);await waitIdle();response=await request('api/execute',{mode:'file',argument:debugPath});assert.equal(response.status,202);await waitFor(current=>current.status==='paused'&&current.debug?.ready);
  refused=await json(await request('api/workspace',{action:'clear-names',names:[kept],confirm:true}));assert.equal(refused.status,400);assert(refused.body.error.includes('salt okunurdur'));
  response=await request('api/debug',{command:'quit'});assert.equal(response.status,200);await waitIdle();response=await request('api/breakpoint',{path:debugPath,line:2,enabled:false});assert.equal(response.status,202);await waitIdle();
  const end=await (await request('api/state')).json();assert.equal(end.epoch,epoch);assert(end.variables.some(item=>item.name===kept));
  await job({action:'clear-names',names:[renamed,logical,character,kept,`${prefix}_debug_a`,`${prefix}_debug_b`].filter(name=>end.variables.some(item=>item.name===name)),confirm:true});
  console.log('HTTP WORKSPACE PASS: typed jobs, injection guards, atomic save, inspected load confirmation, bounded paths, debug read-only, stable epoch.');
 }finally{
  try{let current=await (await request('api/state')).json();if(current.status==='paused'){await request('api/debug',{command:'quit'});await waitIdle();current=await (await request('api/state')).json();}if(current.status==='idle'){let names=[source,renamed,logical,character,kept,`${prefix}_debug_a`,`${prefix}_debug_b`].filter(name=>current.variables.some(item=>item.name===name));if(names.length){await request('api/workspace',{action:'clear-names',names,confirm:true});await waitIdle();}}}catch{}
  try{fs.unlinkSync(mat);}catch{}
  try{fs.unlinkSync(alias);}catch{}
  try{fs.unlinkSync(debugPath);}catch{}
 }
})().catch(error=>{console.error(error);process.exit(1);});
