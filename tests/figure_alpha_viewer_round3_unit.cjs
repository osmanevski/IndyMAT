"use strict";
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),esbuild=require('esbuild');
(async()=>{
 const filename=path.resolve('frontend/figure3d.js');
 const build=await esbuild.build({entryPoints:[filename],bundle:true,write:false,platform:'node',format:'cjs',plugins:[{name:'boundaries',setup(b){
 b.onResolve({filter:/^\.\/(?:figure_webgl|figure_tools|i18n)\.js$/},a=>({path:path.basename(a.path),namespace:'mock'}));
 b.onLoad({filter:/.*/,namespace:'mock'},a=>({contents:{'figure_webgl.js':'export class FigureWebGL {}','figure_tools.js':'export const mountFigureTools=()=>{}; export const unmountFigureTools=()=>{};export const figureCanvasDescription=()=>"";','i18n.js':'export const t=x=>x;export const onLanguageChange=()=>()=>{};'}[a.path]}));
 }}]});
 const m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));m._compile(build.outputFiles[0].text,filename);
 global.document={body:{}};global.devicePixelRatio=1;global.getComputedStyle=()=>({getPropertyValue:()=> '#ffffff'});
 const viewer={disposed:false,width:100,dpr:1,frames:()=>[],ctx:{},data:{axes:[]},tools:{reprojectPins(){assert.fail('disposed viewer continued');}}};
 viewer.renderer={render(){viewer.disposed=true;viewer.ctx=null;viewer.data=null;}};
 assert.doesNotThrow(()=>m.exports.Figure3D.prototype.render.call(viewer));
 const source=fs.readFileSync('frontend/figure3d.js','utf8');
 assert(source.includes('this.canvas.style.visibility = "hidden"'));
 assert(source.includes('this.canvas.style.visibility = "visible"'));
 assert(source.includes('this.renderer.transparent && this.renderer.pending'));
 const fallback=fs.readFileSync('frontend/figures.js','utf8');
 assert(fallback.includes('if (shared.figureData[index] !== fallbackData) return;'));
 assert(fallback.includes('reason_code: code, reason_args: args'));
 console.log('PASS synchronous peeling failure stops disposed viewer render; pending visibility/picking and stale fallback contracts');
})().catch(e=>{console.error(e);process.exitCode=1;});
