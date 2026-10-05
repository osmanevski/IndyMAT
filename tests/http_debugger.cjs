const fs=require('fs');
const path=require('path');
const assert=require('assert');

(async()=>{
 const {url}=JSON.parse(fs.readFileSync('.matlab-free/launch.json'));
 const [base,token]=url.split('#');
 const request=(route,data)=>fetch(base+'api/'+route,{method:data===undefined?'GET':'POST',headers:{'X-MF-Language':'tr','X-MF-Token':token,...data===undefined?{}:{'Content-Type':'application/json'}},body:data===undefined?undefined:JSON.stringify(data)});
 const state=async()=>await (await request('state')).json();
 const wait=async(predicate,label)=>{for(let i=0;i<400;i++){let value=await state();if(predicate(value))return value;await new Promise(resolve=>setTimeout(resolve,25));}throw new Error(label+' timeout: '+JSON.stringify(await state()));};
 const waitIdle=()=>wait(value=>value.status==='idle'||value.status==='dead','idle');
 const waitPaused=(serial=-1)=>wait(value=>value.status==='paused'&&value.debug?.ready&&value.debug.serial!==serial,'paused');
 const suffix=String(Date.now());
 const stem='http_debugger_'+suffix;
 const filename=stem+'.m';
 const file=path.resolve('workspace',filename);
 fs.writeFileSync(file,`function out=${stem}(seed)\n  outer_local=seed+100;\n  out=middle(seed);\n  function mid=middle(value)\n    middle_local=value+10;\n    mid=inner(value);\n  endfunction\n  function answer=inner(value)\n    inner_local=value+1;\n    answer=inner_local*2;\n  endfunction\nendfunction\n`);
 try{
  let response=await request('breakpoints-clear',{});
  assert.equal(response.status,202);
  await waitIdle();
  for(const command of ['continue','step','in','out','quit','frame']){
   response=await request('debug',{command,code:command==='frame'?1:''});
   assert.equal(response.status,400,command+' accepted while idle');
  }
  response=await request('run-to-cursor',{path:file,line:10});
  assert.equal(response.status,400);
  for(const condition of ['value > 0\ndisp(1)',"strcmp(value, 'x)",'x'.repeat(1001)]){
   response=await request('breakpoint',{path:file,line:9,action:'set',condition,enabled:true});
   assert.equal(response.status,400,'invalid condition accepted');
  }
  response=await request('breakpoint',{path:file,line:9,action:'set',condition:'value == 3',enabled:true});
  assert.equal(response.status,202);
  await waitIdle();
  let snapshot=await state();
  assert.deepStrictEqual(snapshot.breakpoints[0].breakpoints,[{line:9,enabled:true,condition:'value == 3'}]);
  response=await request('execute',{mode:'code',code:`${stem}_false=${stem}(2);`});
  assert.equal(response.status,202);
  snapshot=await waitIdle();
  assert(!snapshot.error,snapshot.error);
  response=await request('execute',{mode:'code',code:`${stem}_true=${stem}(3);`});
  assert.equal(response.status,202);
  const {job}=await response.json();
  snapshot=await waitPaused();
  assert.equal(snapshot.debug.line,9);
  assert.equal(snapshot.debug.stack[0].name.split('>').at(-1),'inner');
  assert.equal(snapshot.debug.stack[1].name.split('>').at(-1),'middle');
  assert(snapshot.variables.some(item=>item.name==='value'));
  response=await request('debug',{command:'dbcont'});
  assert.equal(response.status,400,'non-whitelisted debug command accepted');
  response=await request('breakpoint',{path:file,line:10,action:'set',condition:'',enabled:true});
  assert.equal(response.status,400,'paused breakpoint edit started a new job');
  let serial=snapshot.debug.serial;
  response=await request('debug',{command:'frame',code:2});
  assert.equal(response.status,200);
  snapshot=await waitPaused(serial);
  assert.equal(snapshot.debug.stack.find(frame=>frame.current).index,2);
  assert(snapshot.variables.some(item=>item.name==='middle_local'));
  serial=snapshot.debug.serial;
  const epoch=snapshot.epoch;
  response=await request('run-to-cursor',{path:file,line:10});
  assert.equal(response.status,200);
  snapshot=await waitPaused(serial);
  assert.equal(snapshot.job,job,'run-to-cursor started a new job');
  assert.equal(snapshot.epoch,epoch,'run-to-cursor reset the session');
  assert.equal(snapshot.debug.line,10);
  assert(!snapshot.run_to_cursor,'ephemeral breakpoint survived its hit');
  response=await request('stop',{});
  assert.equal(response.status,200);
  await request('stop',{});
  snapshot=await waitIdle();
  assert.equal(snapshot.status,'idle');
  response=await request('breakpoint',{path:file,line:9,action:'disable'});
  assert.equal(response.status,202);
  await waitIdle();
  response=await request('reset',{});
  assert.equal(response.status,200);
  snapshot=await waitIdle();
  assert.equal(snapshot.breakpoints[0].breakpoints[0].condition,'value == 3');
  assert.equal(snapshot.breakpoints[0].breakpoints[0].enabled,false);
  response=await request('breakpoints-clear',{});
  assert.equal(response.status,202);
  snapshot=await waitIdle();
  assert.deepStrictEqual(snapshot.breakpoints,[]);
  console.log('HTTP DEBUGGER PASS: conditions, stack/frame context, real-pause guards, run-to-cursor cleanup, idempotent Stop, reset restore.');
 }finally{
  let current=await state().catch(()=>({}));
  if(['paused','running','starting','stopping'].includes(current.status))await request('stop',{}).catch(()=>{});
  await waitIdle().catch(()=>{});
  await request('breakpoints-clear',{}).catch(()=>{});
  await waitIdle().catch(()=>{});
  await request('execute',{mode:'code',code:`clear ${stem}_false ${stem}_true ${stem};`}).catch(()=>{});
  await waitIdle().catch(()=>{});
  fs.rmSync(file,{force:true});
 }
})().catch(error=>{console.error(error);process.exit(1);});
