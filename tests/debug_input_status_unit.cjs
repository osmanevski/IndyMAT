const fs=require('node:fs');
const assert=require('node:assert/strict');
const source=fs.readFileSync('frontend/console.js','utf8');
const body=source.slice(source.indexOf('function setStatus(s) {'),source.indexOf('registry.setupCommandWindow'));
const en='One reply was sent. Further readers cannot be authenticated; only Stop is available.';
const tr='Bir yanıt gönderildi. Sonraki okuyucular doğrulanamıyor; yalnızca Durdur kullanılabilir.';
const locales={};
for(const file of fs.readdirSync('frontend/locales')) {
 if(!file.endsWith('.js'))continue;
 const m={exports:{}};
 new Function('module',fs.readFileSync('frontend/locales/'+file,'utf8').replace(/^import .*$/gm,'').replace('export default','module.exports ='))(m);
 Object.assign(locales,m.exports);
}
for(const language of ['en','tr']) {
 const elements=new Map();
 const registry={$:id=>{if(!elements.has(id))elements.set(id,{});return elements.get(id);}};
 const buttons=[{}];
 const document={body:{classList:{toggle(){}}},querySelectorAll:()=>buttons};
 const setStatus=new Function('registry','shared','document','t',`let lastStatus; ${body}; return setStatus;`)(registry,{},document,s=>language==='tr'?(locales[s]||s):s);
 registry.$('#status-dot').style={};
 setStatus({status:'running',input_reply_consumed:true,waiting_input:false,debug:{ready:false}});
 assert.equal(registry.$('#status-text').textContent,language==='tr'?tr:en);
 assert.equal(registry.$('#input-bar').hidden,true);
 assert.equal(registry.$('#debug-controls').hidden,true);
 assert.equal(buttons[0].disabled,true);
 assert.equal(registry.$('#stop').disabled,false);
 setStatus({status:'idle',input_reply_consumed:false});
 assert.notEqual(registry.$('#status-text').textContent,language==='tr'?tr:en);
}
console.log('BOUNDED INPUT STATUS PASS (English and Turkish)');
