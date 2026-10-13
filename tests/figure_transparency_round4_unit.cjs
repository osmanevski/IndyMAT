"use strict";
const assert = require('node:assert/strict'), fs = require('node:fs');
const { TransparencyTiming, TIMING_LIMITS } = require('../frontend/figure_transparency_timing.cjs');
const { Peeling, shaderSources } = require('../frontend/figure_peeling.cjs');
const { mock, timers, actualSources, renderer } = require('./figure_peeling_harness.cjs');
let now = 0;
const timing = new TransparencyTiming(() => now);
const complete = elapsed => { const start = timing.begin(); now += elapsed; return timing.complete(start); };
for (const duration of [250.8, 4000]) assert(complete(duration).diagnostics.warmup);
assert.equal(complete(250).diagnostics.slowFrames, 0);
assert.equal(complete(251).diagnostics.slowFrames, 1);
assert.equal(complete(251).diagnostics.slowFrames, 2);
assert.equal(complete(20).diagnostics.slowFrames, 0, 'an isolated stall cannot cause performance fallback');
assert.equal(complete(251).failure, null);
assert.equal(complete(252).failure, null);
assert.deepEqual(complete(253).failure, {actual_ms:253,limit_ms:250,phase:'steady_frames'});
timing.reset(); assert(complete(300).diagnostics.warmup);

const timer = timers(), original = actualSources();
const groups = [{ viewport:[0,0,10,10], draws:[{vao:'mesh',count:3,indexed:true,primitiveBase:1,uniforms:new Map()}] }];
function setup() {
 const gl=mock(), events=[];let now=0;
 const p=new Peeling(gl,shaderSources(original.vertex,original.fragment),{
 pending:()=>events.push(['pending']),ready:d=>events.push(['ready',d]),fail:(c,a)=>events.push([c,a])},()=>now);
 return {p,gl,events,setNow:n=>now=n,render:()=>p.render(groups,10,10,[1,1,1])};
}
try {
 const s=setup();s.gl.texStorage2D=()=>s.setNow(1200);s.render();timer.tick();
 assert.equal(s.events.at(-1)[0],'ready');
 assert.equal(s.events.at(-1)[1].frameMs,0,'target allocation excluded');
 assert.equal(s.events.at(-1)[1].submissionToReadyMs,1200,'setup remains visible in diagnostics');s.p.dispose();
 const cancelled=setup();cancelled.gl.state.available=false;
 for(const t of [0,1500,3000,4500]){cancelled.setNow(t);cancelled.render();}
 assert.equal(cancelled.p.timing.frames,0,'cancelled renders are not warm-up completions');
 cancelled.setNow(5001);timer.tick();
 assert.equal(cancelled.events.at(-1)[1].phase,'blank_timeout');
 assert.equal(cancelled.gl.live.size,0,'repeated cancellation cannot reset the blank deadline');
 const {FigureWebGL}=renderer();let failed;
 const target={transparent:true,disposed:false,fail:(...args)=>failed=args};
 assert.equal(FigureWebGL.prototype.testTransparencyTiming.call(target,[900,800,250.8,250.8,250.8]),true);
 assert.deepEqual(failed,['transparency_time',{actual_ms:250.8,limit_ms:250,phase:'steady_frames'}]);
 assert.equal(FigureWebGL.prototype.testTransparencyTiming.call(target,[NaN]),false);
 assert.equal(timer.tasks.size,0);
} finally {timer.restore();}
// Exercise the real text function in both languages with the measured float.
const source=fs.readFileSync('frontend/figure_tools.js','utf8').replace(/^import .*$/gm,'').replace(/^export /gm,'');
const locale=new Function(fs.readFileSync('frontend/locales/tr_dosyalar.js','utf8').replace('export default','return'))();
for(const language of ['en','tr']){
 const t=(key,args={})=>(language==='tr'?locale[key]:key).replace(/\{(\w+)\}/g,(_,name)=>args[name]);
 const api=new Function('t',source+'\nreturn figureReasonText;')(t);
 const text=api('transparency_time',{actual_ms:250.80000001192093,limit_ms:250});
 assert(text.includes('251 ms'));assert(!text.includes('250.8'));assert(text.includes('250 ms'));
}
assert.equal(TIMING_LIMITS.blankMs,5000);
console.log('PASS injected completion clock, setup exclusion, warm-up/stall/reset policy, cancellation deadline, deterministic fallback hook and EN/TR rounded ms');
