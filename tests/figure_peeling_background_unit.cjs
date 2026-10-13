"use strict";
const assert=require("node:assert/strict");
const {Peeling,shaderSources}=require("../frontend/figure_peeling.cjs");
const {mock,actualSources,timers}=require("./figure_peeling_harness.cjs");
const original=actualSources(),sources=shaderSources(original.vertex,original.fragment);
assert.match(sources.fullFragment,/outputColor = resolve \? p : vec4\(p.rgb\*p.a,p.a\)/,"resolve preserves premultiplied RGB and transmittance for destination blending");
const timer=timers(),gl=mock(),p=new Peeling(gl,sources,{pending(){},ready(){},fail(){assert.fail("fallback");}});
try {
 const draw=decoration=>({decoration,uniforms:new Map(),vao:null,indexed:false,count:3,primitiveBase:1});
 p.render([{viewport:[0,0,100,100],draws:[draw(true),draw(false)]}],100,100,[1,1,1]);
 const output=gl.calls.filter(c=>/^draw/.test(c.method)&&c.framebuffer===p.output.fbo);
 assert.deepEqual(output.map(c=>c.program),[p.overlay,p.full],"decoration precedes compositing geometry in the output framebuffer");
 const geometry=gl.calls.filter(c=>/^draw/.test(c.method)&&c.program===p.peel);
 assert.equal(geometry.length,9,"only geometry participates in 8 layers and overflow query");
 const composite=gl.calls.findIndex(c=>c===output[1]);
 assert(gl.calls.slice(0,composite).some(c=>c.method==="blendFuncSeparate"&&c.args.join()==="ONE,SRC_ALPHA,ZERO,ONE"),"premultiplied geometry plus transmittance times decorated destination");
 timer.tick();
} finally {p.dispose();timer.restore();}
console.log("PASS decorated background under peeled geometry; decoration outside layer bound");
