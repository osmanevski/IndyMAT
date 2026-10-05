const fs=require('fs'),path=require('path'),assert=require('assert'),http=require('http'),os=require('os');
const {randomUUID}=require('node:crypto');

(async()=>{
 const launch=JSON.parse(fs.readFileSync('.matlab-free/launch.json','utf8'));
 const [base,token]=launch.url.split('#');
 const request=(route,body,headers={})=>fetch(base+route,{method:body===undefined?'GET':'POST',headers:{'X-MF-Language':'tr','X-MF-Token':token,...body===undefined?{}:{'Content-Type':'application/json'},...headers},body:body===undefined?undefined:JSON.stringify(body)});
 const waitIdle=async()=>{for(let index=0;index<200;index++){let state=await (await request('api/state')).json();if(state.status==='idle')return state;await new Promise(resolve=>setTimeout(resolve,50));}throw new Error('Octave job timeout');};
 const initial=await waitIdle();
 const marker='http-file-ops-'+randomUUID();
 const root=path.join(initial.workspace,marker);
 const outside=path.join(os.tmpdir(),'outside-'+marker);
 let trashed=[];
 let trashCollision='';
 fs.mkdirSync(root);
 fs.mkdirSync(outside);
 const operation=(operation,fields={},headers={})=>request('api/file-operation',{operation,...fields},headers);
 try{
  assert.equal((await fetch(base+'api/file-operation',{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'})).status,403);
  assert.equal((await operation('inspect',{source:root},{Origin:'https://evil.example'})).status,403);
  const badHost=await new Promise((resolve,reject)=>{let target=new URL(base+'api/file-operation');let req=http.request({hostname:target.hostname,port:target.port,path:target.pathname,method:'POST',headers:{Host:'evil.example','X-MF-Language':'tr','X-MF-Token':token,'Content-Type':'application/json'}},response=>{response.resume();resolve(response.statusCode);});req.on('error',reject);req.end(JSON.stringify({operation:'inspect',source:root}));});
  assert.equal(badHost,403);
  let before=await (await request('api/state')).json();
  let response=await operation('create_file',{source:root,name:'ilk.m'});assert.equal(response.status,200);let made=await response.json();assert.equal(made.path,path.join(root,'ilk.m'));assert.equal(typeof made.hash,'string');
  response=await operation('rename',{source:made.path,name:'ikinci.m'});assert.equal(response.status,200);let renamed=await response.json();assert.equal(renamed.path,path.join(root,'ikinci.m'));assert.equal(renamed.hash,made.hash);
  response=await operation('duplicate',{source:renamed.path,name:'kopya.m'});assert.equal(response.status,200);let copied=await response.json();assert(fs.existsSync(copied.path));
  response=await operation('create_folder',{source:root,name:'hedef'});assert.equal(response.status,200);let target=(await response.json()).path;
  response=await operation('move',{source:copied.path,destination:target});assert.equal(response.status,200);let moved=await response.json();assert.equal(moved.path,path.join(target,'kopya.m'));
  let unchanged=await (await request('api/state')).json();assert.equal(unchanged.job,before.job);assert.equal(unchanged.cwd,before.cwd);
  fs.writeFileSync(path.join(root,'çakışma.m'),'koru');
  assert.equal((await operation('rename',{source:renamed.path,name:'çakışma.m'})).status,409);assert.equal(fs.readFileSync(path.join(root,'çakışma.m'),'utf8'),'koru');assert(fs.existsSync(renamed.path));
  for(const name of ['.','..','...','a/b','a\\b','.hidden.m','control\t.m','format\u200b.m','x'.repeat(256)])assert.equal((await operation('rename',{source:renamed.path,name})).status,400,name);
  // Finding 1: stale editor hash must reject before relocation.
  fs.writeFileSync(renamed.path,'external B');
  for(const [op,fields] of [['rename',{name:'stale.m'}],['move',{destination:target}]]){
   const stale=await operation(op,{source:renamed.path,...fields,expected:[{path:renamed.path,hash:renamed.hash}]});
   assert.equal(stale.status,409);assert.equal(fs.readFileSync(renamed.path,'utf8'),'external B');
  }
  // Finding 10: spelling change works on the actual test volume.
  let caseResponse=await operation('rename',{source:renamed.path,name:'IKINCI.m'});assert.equal(caseResponse.status,200);let upper=await caseResponse.json();
  caseResponse=await operation('rename',{source:upper.path,name:'ikinci.m'});assert.equal(caseResponse.status,200);assert(fs.readdirSync(root).includes('ikinci.m'));
  assert.equal((await request('api/file-operation',{operation:'rename',source:renamed.path,name:'x.m',extra:true})).status,400);
  assert.equal((await operation('inspect',{source:'../'+marker})).status,403);
  assert.equal((await operation('inspect',{source:'/etc/passwd'})).status,403);
  const sourceLink=path.join(root,'source-link');fs.symlinkSync(renamed.path,sourceLink);assert.equal((await operation('inspect',{source:sourceLink})).status,403);
  const middle=path.join(root,'middle');fs.symlinkSync(outside,middle);fs.writeFileSync(path.join(outside,'outside.m'),'x');assert.equal((await operation('inspect',{source:path.join(middle,'outside.m')})).status,403);
  const destinationLink=path.join(root,'destination-link');fs.symlinkSync(target,destinationLink);assert.equal((await operation('move',{source:renamed.path,destination:destinationLink})).status,403);
  const full=path.join(root,'dolu');fs.mkdirSync(full);fs.writeFileSync(path.join(full,'a.txt'),'a');fs.mkdirSync(path.join(full,'alt'));fs.writeFileSync(path.join(full,'alt','b.txt'),'b');
  response=await operation('inspect',{source:full});assert.equal(response.status,200);let info=await response.json();assert.equal(info.item_count,3);assert.equal(info.directory,true);
  response=await operation('duplicate',{source:full,name:'dolu-kopya'});assert.equal(response.status,200);let folderCopy=await response.json();assert.equal(folderCopy.item_count,3);assert(fs.existsSync(path.join(folderCopy.path,'alt','b.txt')));
  response=await operation('rename',{source:full,name:'trash-'+marker});assert.equal(response.status,200);let folderRenamed=await response.json();assert.equal(folderRenamed.item_count,3);
  response=await operation('move',{source:folderRenamed.path,destination:target});assert.equal(response.status,200);let folderMoved=await response.json();assert.equal(folderMoved.item_count,3);
  const sessionFolder=path.join(root,'oturum');fs.mkdirSync(sessionFolder);
  response=await request('api/folder',{path:sessionFolder});assert.equal(response.status,202);await waitIdle();let sessionBefore=await (await request('api/state')).json();
  response=await operation('rename',{source:sessionFolder,name:'yasak'});assert.equal(response.status,403);let refusal=await response.json();assert(refusal.error.includes('Octave oturumunun'));
  let sessionAfter=await (await request('api/state')).json();assert.equal(sessionAfter.cwd,sessionBefore.cwd);assert.equal(sessionAfter.job,sessionBefore.job);assert(fs.existsSync(sessionFolder));
  response=await request('api/folder',{path:initial.workspace});assert.equal(response.status,202);let restored=await waitIdle();
  const collisionCandidate=path.join(os.homedir(),'.Trash',path.basename(folderMoved.path));fs.writeFileSync(collisionCandidate,'çakışma',{flag:'wx'});trashCollision=collisionCandidate;
  response=await operation('trash',{source:folderMoved.path});assert.equal(response.status,200);let body=await response.json();trashed.push(body.path);assert.equal(body.item_count,3);assert(fs.existsSync(body.path));assert.equal(path.basename(body.path),path.basename(folderMoved.path)+' 2');assert.equal(fs.readFileSync(trashCollision,'utf8'),'çakışma');
  let after=await (await request('api/state')).json();assert.equal(after.cwd,restored.cwd);assert.equal(after.job,restored.job);assert(fs.existsSync(renamed.path));
  // Findings 5/9: real breakpoint job, prefix relocation, paused and input cwd.
  const bpFolder=path.join(root,'breakpoints');fs.mkdirSync(bpFolder);
  const bp=path.join(bpFolder,'fileops_probe.m');fs.writeFileSync(bp,"disp(1);\ndisp(2);\n");
  response=await request('api/breakpoint',{path:bp,line:2,enabled:true});assert.equal(response.status,202);await waitIdle();
  response=await operation('rename',{source:bpFolder,name:'breakpoints_new'});assert.equal(response.status,200);let bpMoved=await response.json();assert(bpMoved.breakpoint_job);
  let bpState=await waitIdle();let newBp=path.join(bpMoved.path,'fileops_probe.m');assert(bpState.breakpoints.some(item=>item.file===newBp));assert(!bpState.breakpoints.some(item=>item.file.startsWith(bpFolder+'/')));
  response=await request('api/execute',{mode:'file',argument:newBp});assert.equal(response.status,202);
  for(let i=0;i<200;i++){bpState=await (await request('api/state')).json();if(bpState.status==='paused'&&bpState.debug?.ready)break;await new Promise(resolve=>setTimeout(resolve,50));}
  assert.equal(bpState.status,'paused');
  assert.equal((await operation('rename',{source:bpMoved.path,name:'blocked'})).status,400);
  await request('api/debug',{command:'continue'});await waitIdle();
  response=await operation('trash',{source:bpMoved.path});assert.equal(response.status,200);let bpTrashed=await response.json();trashed.push(bpTrashed.path);bpState=await waitIdle();assert(!bpState.breakpoints.some(item=>item.file===newBp));
  const quote=value=>"'"+value.replaceAll("'","''")+"'";
  response=await request('api/execute',{mode:'code',code:`cd(${quote(sessionFolder)}); input('file ops test: ','s'); cd(${quote(initial.workspace)}); clear ans;`});assert.equal(response.status,202);
  for(let i=0;i<200;i++){bpState=await (await request('api/state')).json();if(bpState.waiting_input)break;await new Promise(resolve=>setTimeout(resolve,50));}
  assert(bpState.waiting_input);
  for(const [op,fields] of [['rename',{name:'blocked'}],['move',{destination:target}],['trash',{}]])assert.equal((await operation(op,{source:sessionFolder,...fields})).status,400);
  await request('api/input',{text:'done'});await waitIdle();
  console.log('HTTP FILE OPS PASS: route guards, strict schemas, path/symlink boundaries, collisions, folder counts, session cwd refusal, refreshable results and recoverable Trash move.');
 }finally{
  try{let state=await (await request('api/state')).json();if(state.status!=='idle'){await request('api/stop',{});await waitIdle();}}catch{}
  try{let state=await (await request('api/state')).json();if(state.cwd!==initial.workspace){await request('api/folder',{path:initial.workspace});await waitIdle();}}catch{}
  for(const item of trashed)fs.rmSync(item,{recursive:true,force:true});
  if(trashCollision)fs.rmSync(trashCollision,{force:true});
  fs.rmSync(root,{recursive:true,force:true});
  fs.rmSync(outside,{recursive:true,force:true});
 }
})().catch(error=>{console.error(error);process.exit(1);});
