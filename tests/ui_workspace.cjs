const assert=require('assert');
const fs=require('fs');
const path=require('path');
const {randomUUID}=require('crypto');
const {chromium}=require('playwright');

(async()=>{
 const launch=JSON.parse(fs.readFileSync('.matlab-free/launch.json','utf8'));
 const [base,token]=launch.url.split('#');
 const suffix=randomUUID().replaceAll('-','');
 const a='ui_ws_a_'+suffix,m='ui_ws_m_'+suffix,z='ui_ws_z_'+suffix,renamed='ui_ws_renamed_'+suffix;
 const request=(endpoint,body)=>fetch(base+endpoint,{method:body===undefined?'GET':'POST',headers:{'X-MF-Token':token,...body===undefined?{}:{'Content-Type':'application/json'}},body:body===undefined?undefined:JSON.stringify(body)});
 const waitIdle=async()=>{for(let index=0;index<300;index++){let state=await (await request('api/state')).json();if(state.status==='idle')return state;await new Promise(resolve=>setTimeout(resolve,50));}throw new Error('UI workspace setup timeout');};
 let response=await request('api/execute',{mode:'code',code:`${a}=1;${m}=2;${z}=3;`});assert.equal(response.status,202);let setup=await waitIdle();
 const mat=path.join(setup.current,'ui-workspace-'+suffix+'.mat');
 const browser=await chromium.launch({headless:true});
 const context=await browser.newContext({ locale: "tr-TR" });
 const page=await context.newPage();
 try{
  await page.goto(launch.url);
  await page.waitForFunction(name=>document.querySelector(`#variables tr[data-name="${name}"]`),a);
  const row=name=>page.locator(`#variables tr[data-name="${name}"]`);
  assert.equal(await row(a).count(),1);
  await row(a).click();
  assert.equal(await page.locator('#modal[open]').count(),0,'single click must only select');
  assert.equal(await row(a).getAttribute('aria-selected'),'true');
  assert.equal(await row(a).locator('td').count(),4);
  assert.equal(await row(a).locator('.shape i').count(),9);
  assert.equal(await row(a).locator('td').nth(2).innerText(),'1×1');
  assert.equal(await page.locator('#variables tr').filter({hasText:a}).count(),1);
  for(const cellIndex of [0,1,2,3]){
   await row(a).locator('td').nth(cellIndex).dblclick();
   await page.locator('#modal[open]').waitFor();
   assert.equal(await page.locator('#modal-title').innerText(),`${a} — Değişken görünümü`);
   assert.equal(await page.locator('#variables .workspace-inline').count(),0,'double click must inspect, not edit');
   await page.locator('#modal-close').click();
  }
  await row(a).focus();
  await row(a).press('Enter');
  await page.locator('#modal[open]').waitFor();
  assert.equal(await page.locator('#modal-title').innerText(),`${a} — Değişken görünümü`);
  await page.locator('#modal-close').click();
  const names=()=>page.locator('#variables tr').evaluateAll(rows=>rows.map(row=>row.dataset.name));
  let ordered=(await names()).filter(name=>[a,m,z].includes(name));assert.deepEqual(ordered,[a,m,z]);
  await page.locator('.workspace-sort[data-key="name"]').click();ordered=(await names()).filter(name=>[a,m,z].includes(name));assert.deepEqual(ordered,[z,m,a]);
  await page.locator(`#variables tr[data-name="${z}"]`).click();
  await page.locator(`#variables tr[data-name="${m}"]`).click({modifiers:['Meta']});
  assert.equal(await page.locator('#variables tr.selected').count(),2);
  await page.locator(`#variables tr[data-name="${a}"]`).click();
  await page.locator(`#variables tr[data-name="${m}"]`).click({modifiers:['Shift']});
  assert.equal(await page.locator('#variables tr.selected').count(),2);
  await row(a).click();
  await row(a).press('F2');
  let inline=page.getByRole('textbox',{name:'Yeni değişken adı',exact:true});
  assert.equal(await inline.count(),1);
  await inline.fill(renamed);
  await inline.evaluate(input=>{window.workspaceEditProbe=input;});
  const external=await request('api/execute',{mode:'code',code:`${m}=27;`});
  assert.equal(external.status,202);
  const externalJob=(await external.json()).job;
  await page.waitForResponse(async response=>{
   if(!response.url().endsWith('/api/state'))return false;
   const state=await response.json();
   return state.job===externalJob&&state.status==='idle';
  });
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  assert.equal(await inline.inputValue(),renamed,'poll must preserve rename draft');
  assert(await inline.evaluate(input=>input===window.workspaceEditProbe&&input===document.activeElement),'poll replaced or blurred inline editor');
  await inline.press('Enter');
  await page.waitForFunction(name=>document.querySelector(`#variables tr[data-name="${name}"]`),renamed);
  await row(renamed).click();
  await page.locator('#workspace-edit-value').click();
  inline=page.getByRole('textbox',{name:`${renamed} skaler değeri`,exact:true});
  assert.equal(await inline.count(),1);
  await inline.fill('42.5');await inline.press('Enter');
  await page.waitForFunction(name=>document.querySelector(`#variables tr[data-name="${name}"] td.workspace-value`)?.textContent==='42.5',renamed);
  await page.locator(`#variables tr[data-name="${renamed}"]`).click();
  await page.locator('#workspace-save-selection').click();
  await page.getByRole('textbox',{name:'MAT dosyası yolu',exact:true}).fill(mat);await page.locator('.workspace-path-dialog button[type="submit"]').click();
  for(let i=0;i<150&&!fs.existsSync(mat);i++)await page.waitForTimeout(100);assert(fs.existsSync(mat));await page.waitForFunction(()=>document.querySelector('#status-text').textContent==='Hazır');
  await row(renamed).click();
  await page.locator('#workspace-edit-value').click();
  inline=page.getByRole('textbox',{name:`${renamed} skaler değeri`,exact:true});await inline.fill('99');await inline.press('Enter');
  await page.waitForFunction(name=>document.querySelector(`#variables tr[data-name="${name}"] td.workspace-value`)?.textContent==='99',renamed);
  let loadConfirmation='';
  page.once('dialog',async dialog=>{loadConfirmation=dialog.message();await dialog.accept();});
  await page.locator('#workspace-load').click();await page.getByRole('textbox',{name:'MAT dosyası yolu',exact:true}).fill(mat);await page.locator('.workspace-path-dialog button[type="submit"]').click();
  await page.waitForFunction(name=>document.querySelector(`#variables tr[data-name="${name}"] td.workspace-value`)?.textContent==='42.5',renamed);
  assert(loadConfirmation.includes(renamed),loadConfirmation);
  await page.locator(`#variables tr[data-name="${z}"]`).click();await page.locator(`#variables tr[data-name="${z}"]`).press('Shift+ArrowDown');
  assert((await page.locator('#variables tr.selected').count())>=2);
  let deleteConfirmation='';page.once('dialog',async dialog=>{deleteConfirmation=dialog.message();await dialog.accept();});await page.locator('#workspace-delete').click();
  await page.waitForFunction(name=>!document.querySelector(`#variables tr[data-name="${name}"]`),z);assert(deleteConfirmation.includes(z));
  console.log('UI WORKSPACE PASS: selection, double-click/Enter inspection, F2 rename, toolbar scalar edit, sort, delete, save and load.');
 }finally{
  await browser.close();
  try{fs.unlinkSync(mat);}catch{}
  let state=await waitIdle().catch(()=>null);
  if(state){let names=[a,m,z,renamed].filter(name=>state.variables.some(item=>item.name===name));if(names.length){await request('api/workspace',{action:'clear-names',names,confirm:true});await waitIdle();}}
 }
})().catch(error=>{console.error(error);process.exit(1);});
