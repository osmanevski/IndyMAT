const fs=require('fs');
const path=require('path');
const assert=require('assert');
const {chromium}=require('playwright');

(async()=>{
 const launch=JSON.parse(fs.readFileSync('.matlab-free/launch.json'));
 const [base,token]=launch.url.split('#');
 const suffix=String(Date.now());
 const stem='ui_debugger_'+suffix;
 const name=stem+'.m';
 const file=path.resolve('workspace',name);
 fs.writeFileSync(file,`function out=${stem}(seed)\n  outer_local=seed+100;\n  out=middle(seed);\n  function mid=middle(value)\n    middle_local=value+10;\n    mid=inner(value);\n  endfunction\n  function answer=inner(value)\n    inner_local=value+1;\n    answer=inner_local*2;\n  endfunction\nendfunction\n`);
 const api=(route,data)=>fetch(base+'api/'+route,{method:data===undefined?'GET':'POST',headers:{'X-MF-Token':token,...data===undefined?{}:{'Content-Type':'application/json'}},body:data===undefined?undefined:JSON.stringify(data)});
 const state=async()=>await (await api('state')).json();
 const waitState=async(predicate)=>{
  for(let i=0;i<500;i++){
   const value=await state();
   if(predicate(value))return value;
   await new Promise(resolve=>setTimeout(resolve,25));
  }
  throw new Error('Engine state timeout: '+JSON.stringify(await state()));
 };
 const waitIdle=job=>waitState(value=>value.status==='idle'&&(!job||value.job===job));
 const waitPaused=(job,serial=-1)=>waitState(value=>value.status==='paused'&&value.job===job&&value.debug?.ready&&value.debug.serial>serial);
 const errors=[];
 let browser=null,context=null;
 try{
  const cleared=await api('breakpoints-clear',{});
  assert.equal(cleared.status,202);
  await waitIdle((await cleared.json()).job);
  browser=await chromium.launch({headless:true});
  context=await browser.newContext({ locale: "tr-TR" });
  const page=await context.newPage();
  const act=async(route,action,status=202)=>{
   const responsePromise=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/'+route&&response.request().method()==='POST');
   await action();
   const response=await responsePromise;
   assert.equal(response.status(),status,route+': '+await response.text());
   return response.json();
  };
  page.on('pageerror',error=>errors.push(error.message));
  await page.goto(launch.url);
  await page.waitForFunction(()=>document.querySelector('#status-text')?.textContent==='Hazır');
  await page.locator('#refresh-files').click();
  await page.locator('#files').getByRole('button',{name,exact:true}).click();
  await page.waitForFunction(expected=>document.querySelector('#editor-label')?.textContent===expected,name);
  const gutter=page.locator('#editor .cm-breakpoint-gutter .cm-gutterElement');
  let change=await act('breakpoint',()=>gutter.nth(8).click());
  await waitIdle(change.job);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır'&&!document.querySelector('#debugger-panel').hidden&&document.querySelector('#debug-breakpoint-count').textContent==='1');
  assert.equal(await page.locator('#debug-breakpoints .debug-breakpoint-location').textContent(),'Satır 9');
  page.once('dialog',dialog=>dialog.accept('value == 3'));
  change=await act('breakpoint',()=>gutter.nth(8).click({button:'right'}));
  await waitIdle(change.job);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır'&&document.querySelector('.debug-breakpoint-condition')?.textContent==='value == 3');
  assert.equal(await page.locator('#editor .cm-breakpoint-marker.conditional').count(),1);
  let enabled=page.locator('#debug-breakpoints .debug-breakpoint-row input[type=checkbox]');
  change=await act('breakpoint',()=>enabled.uncheck());
  await waitIdle(change.job);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır'&&document.querySelector('.debug-breakpoint-row')?.classList.contains('disabled'));
  change=await act('breakpoint',()=>enabled.check());
  await waitIdle(change.job);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır'&&!document.querySelector('.debug-breakpoint-row')?.classList.contains('disabled'));
  await page.locator('#command').fill(`${stem}_false=${stem}(2);`);
  change=await act('execute',()=>page.locator('#command').press('Enter'));
  let runtimeState=await waitIdle(change.job);
  assert(!runtimeState.error,runtimeState.error);
  assert.equal(runtimeState.variables.find(item=>item.name===stem+'_false')?.preview,'6');
  await page.waitForFunction(variable=>document.querySelector('#status-text').textContent==='Hazır'&&document.querySelector('#variables').innerText.includes(variable),stem+'_false');
  await page.locator('#command').fill(`${stem}_true=${stem}(3);`);
  change=await act('execute',()=>page.locator('#command').press('Enter'));
  const job=change.job;
  runtimeState=await waitPaused(job);
  assert.equal(runtimeState.debug.line,9);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Kesme noktasında'&&document.querySelectorAll('#debug-stack .debug-frame').length>=3&&!document.querySelector('#run-to-cursor').disabled);
  assert.equal(await page.locator('#debug-stack .debug-frame').first().getAttribute('aria-current'),'true');
  await page.waitForSelector('#editor .cm-debug-line');
  assert.equal(await page.locator('#editor .cm-debug-line').count(),1);
  let serial=runtimeState.debug.serial;
  await act('debug',()=>page.locator('#debug-stack .debug-frame[data-frame="2"]').click(),200);
  runtimeState=await waitPaused(job,serial);
  assert.equal(runtimeState.debug.frame,2);
  await page.waitForFunction(()=>document.querySelector('#debug-stack .debug-frame[data-frame="2"]')?.getAttribute('aria-current')==='true'&&document.querySelector('#variables').innerText.includes('middle_local')&&document.querySelector('#cursor').textContent.startsWith('Satır 6,'));
  await page.locator('#editor .cm-line').filter({hasText:'answer=inner_local*2;'}).click();
  serial=runtimeState.debug.serial;
  const epoch=runtimeState.epoch;
  await act('run-to-cursor',()=>page.locator('#run-to-cursor').click(),200);
  runtimeState=await waitPaused(job,serial);
  assert.equal(runtimeState.debug.line,10);
  assert.equal(runtimeState.epoch,epoch);
  assert(!runtimeState.run_to_cursor);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Kesme noktasında'&&document.querySelector('#cursor').textContent.startsWith('Satır 10,')&&document.querySelector('#editor .cm-debug-line')?.textContent.includes('answer=inner_local*2;'));
  serial=runtimeState.debug.serial;
  await act('debug',()=>page.locator('#debug-stack .debug-frame[data-frame="2"]').click(),200);
  runtimeState=await waitPaused(job,serial);
  assert.equal(runtimeState.debug.frame,2);
  await page.waitForFunction(()=>document.querySelector('#debug-stack .debug-frame[data-frame="2"]')?.getAttribute('aria-current')==='true'&&document.querySelector('#variables').innerText.includes('middle_local')&&document.querySelector('#editor .cm-debug-frame-line')?.textContent.includes('mid=inner(value);'));
  assert(await page.locator('#editor .cm-debug-frame-line').count());
  await act('stop',()=>page.locator('#stop').click(),200);
  await waitIdle(job);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır');
  change=await act('breakpoint',()=>page.locator('#debug-breakpoints .debug-breakpoint-remove').click());
  await waitIdle(change.job);
  await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır'&&document.querySelector('#debugger-panel').hidden);
  assert.deepStrictEqual(errors,[]);
  console.log('UI DEBUGGER PASS: gutter condition, grouped panel controls, stack frame context, editor indicators, run-to-cursor, Stop.');
 }finally{
  if(context)await context.close();
  if(browser)await browser.close();
  await api('stop',{}).catch(()=>{});
  for(let i=0;i<200;i++){
   let response=await api('state').catch(()=>null);
   let current=response&&await response.json();
   if(!current||['idle','dead'].includes(current.status))break;
   await new Promise(resolve=>setTimeout(resolve,25));
  }
  try{
   const cleared=await api('breakpoints-clear',{});
   if(cleared.status===202)await waitIdle((await cleared.json()).job);
   const cleaned=await api('execute',{mode:'code',code:`clear ${stem}_false ${stem}_true ${stem};`});
   if(cleaned.status===202)await waitIdle((await cleaned.json()).job);
  }finally{
   fs.rmSync(file,{force:true});
  }
 }
})().catch(error=>{console.error(error);process.exit(1);});
