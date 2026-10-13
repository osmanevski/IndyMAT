"use strict";
// Synthetic protocol cases; these are not recorded Octave graphics fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const esbuild = require('esbuild');
const {validate} = require('../frontend/figure_data_utils.cjs');
const {triangulatePatch,patchPoint} = require('../frontend/figure_patch3d_utils.cjs');
const colors = require('../frontend/figure_color_utils.cjs');
const geometry = require('../frontend/figure_geometry_utils.cjs');
const camera = require('../frontend/figure_camera_utils.cjs');
const picking = require('../frontend/figure_pick_utils.cjs');
const {tipText} = require('../frontend/figure_tool_utils.cjs');
const descriptor = (shape,values) => ({shape,values,order:'column-major'});
const matrix = rows => descriptor([rows.length,rows[0]?.length ?? 3],Array.from({length:rows[0]?.length ?? 3},(_,c)=>rows.map(row=>row[c])).flat());
const constant = rgb=>({mode:'constant',association:'constant',rgb});
const none = ()=>({mode:'none',association:'constant'});
const flat = (values,encoding='indexed',mapping='scaled',name='double')=>({mode:'flat',association:'face',encoding,mapping,cdata_class:name,data:descriptor(encoding==='indexed'?[values.length,1]:[values.length/3,1,3],values)});
function figure() {
 const f=JSON.parse(fs.readFileSync('tests/fixtures/figure_v3/surf_vector.json'));
 const a=f.axes[0];a.xlim=[0,3];a.ylim=[0,1];a.zlim=[0,2];a.clim=[1,2];a.view=[0,90];a.visible=false;
 a.colormap=matrix([[1,0,0],[0,0,1]]);a.decimated=false;
 Object.assign(a.camera,{position:[1.5,.5,10],target:[1.5,.5,1],up_vector:[0,1,0]});
 const vertices=[[0,0,1],[1,0,1],[1,1,1],[0,1,1],[2,0,1],[3,0,1],[3,1,1],[2,1,1]];
 const faces=[[0,1,2,3],[4,5,6,7]],g=triangulatePatch(vertices,faces);
 a.series=[{id:0,kind:'patch3d',role:'patch',index_base:0,triangulation:'first-valid-ear-v1',polygon_vertex_limit:64,
 vertices:matrix(vertices),faces:matrix(faces),triangle_indices:matrix(g.triangles),triangle_faces:g.faceOfTriangle,
 edge_indices:matrix(g.edges),triangle_count:g.triangles.length,face_color:flat([1,2]),edge_color:constant([.1,.1,.1]),
 line_style:'-',line_width:2,display_name:'Original faces',decimated:false,original_points:8,rendered_points:8}];
 f.vertex_count=8;f.triangle_count=4;f.decimated=false;return f;
}
let cases=0;
const test=(name,fn)=>{fn();cases++;console.log('PASS '+name);};
test('stage4 literal i18n keys and original-value tip labels',()=>{
 const source=fs.readFileSync('frontend/locales/tr_dosyalar.js','utf8');
 const locale=new Function(source.replace('export default','return'))();
 for(const [key,value] of [['Level','Seviye'],['Face','Yüz'],['Vertex','Köşe'],
 ['Only the PNG view is available: filled contours.','Yalnızca PNG görünümü kullanılabilir: dolgulu konturlar.'],
 ['Only the PNG view is available: contour labels.','Yalnızca PNG görünümü kullanılabilir: kontur etiketleri.']])assert.equal(locale[key],value);
 assert.match(tipText({x:1,y:2,z:3,face:0,vertex:2,level:4},{face:locale.Face,vertex:locale.Vertex,level:locale.Level}),/Yüz: 1\nKöşe: 3\nSeviye: 4/);
});
test('validator/scene preserve original doubles, face/vertex owners and source-only edges',()=>{
 const f=figure(),before=structuredClone(f),result=validate(f);assert(result.ok,JSON.stringify(result));
 const scene=geometry.buildScene(f.axes[0]);assert.deepEqual(f,before);
 const mesh=scene.meshes[0];assert.equal(mesh.indices.length,12);assert.deepEqual([...mesh.cellOfTriangle],[0,0,1,1]);
 assert.deepEqual([...mesh.source.vertexIndices],[3,0,1,1,2,3,7,4,5,5,6,7]);
 assert.equal(scene.edges[0].positions.length,48);assert.equal(scene.counts.edges,8);
 for(let i=0;i<6;i++)assert.deepEqual([...mesh.colors.subarray(i*3,i*3+3)],[1,0,0]);
 for(let i=6;i<12;i++)assert.deepEqual([...mesh.colors.subarray(i*3,i*3+3)],[0,0,1]);
});
for(const mapping of ['scaled','direct'])for(const name of ['double','single','uint8','int16'])test(`per-face indexed ${mapping}/${name}`,()=>{
 const f=figure(),s=f.axes[0].series[0];s.face_color=flat(['uint8','int16'].includes(name)&&mapping==='direct'?[0,1]:[1,2],'indexed',mapping,name);
 assert(validate(f).ok);assert.deepEqual(colors.patchColor(s.face_color,0,f.axes[0]),[1,0,0]);assert.deepEqual(colors.patchColor(s.face_color,1,f.axes[0]),[0,0,1]);
 assert.equal(colors.rawPatchColor(s,1),s.face_color.data.values[1]);
});
test('per-face RGB and scalar singleton flat colours',()=>{
 const f=figure(),s=f.axes[0].series[0];s.face_color=flat([1,0,0,0,0,1],'truecolor');assert(validate(f).ok);
 assert.deepEqual(colors.patchColor(s.face_color,0,f.axes[0]),[1,0,0]);assert.deepEqual(colors.patchColor(s.face_color,1,f.axes[0]),[0,0,1]);
 assert.deepEqual(colors.rawPatchColor(s,1),[0,0,1]);s.face_color=flat([1]);assert(validate(f).ok);assert.deepEqual(colors.patchColor(s.face_color,1,f.axes[0]),[1,0,0]);
});
test('picking uses original face/vertex and opaque depth; wire edges also tip',()=>{
 const f=figure(),a=f.axes[0],s=a.series[0],scene=geometry.buildScene(a),cam=camera.createCamera(a),vp={width:600,height:400};
 const point=camera.dataToNormalised(a,[.9,.9,1]),pixel=camera.project(cam,vp,point),hit=picking.pick(scene,cam,vp,pixel.x,pixel.y);
 assert(hit);assert.equal(hit.kind,'patch3d');assert.equal(hit.face,0);assert.equal(hit.vertex,2);assert.deepEqual(hit.data,{x:1,y:1,z:1});assert.equal(hit.colorValue,1);
 assert.match(tipText({...hit.data,face:hit.face,vertex:hit.vertex}),/Face: 1\nVertex: 3/);
 const source=patchPoint(s,2);assert.deepEqual(source,[1,1,1]);
 // Add a line behind the patch: its exact screen centre must stay occluded.
 const sample={kind:'line',x:[1,.9],y:[1,.9],z:[.2,.2],source_indices:[0,1]};
 const linepositions=new Float32Array([...camera.dataToNormalised(a,[1,1,.2]),...camera.dataToNormalised(a,[.9,.9,.2])]);
 scene.lines.push({positions:linepositions,series:1,width:1,breaks:new Uint32Array([0,2]),sourceIndices:new Uint32Array([0,1]),source:{series:sample,sampleIndices:new Uint32Array([0,1])}});
 assert.equal(picking.pick(scene,cam,vp,pixel.x,pixel.y,{radiusPixels:20}).kind,'patch3d');
 s.face_color=none();const wire=geometry.buildScene(a),vertexPixel=camera.project(cam,vp,camera.dataToNormalised(a,source));
 const edgeHit=picking.pick(wire,cam,vp,vertexPixel.x,vertexPixel.y);assert(edgeHit);assert.equal(edgeHit.vertex,2);assert.equal(edgeHit.face,0);
});
for(const [name,mutate,reason] of [
 ['diagonal edge',s=>s.edge_indices.values[0]=2,'invalid_index'],
 ['invented triangle',s=>s.triangle_indices.values[0]=2,'invalid_index'],
 ['face owner',s=>s.triangle_faces[0]=1,'invalid_index'],
 ['bad index',s=>s.faces.values[0]=8,'invalid_index'],
 ['NaN before valid face index',s=>s.faces.values[0]=null,'invalid_index'],
 ['nonfinite vertex',s=>s.vertices.values[0]=null,'invalid_data'],
 ['nonplanar',s=>s.vertices.values[3+16]=1.2,'unsupported_patch'],
 ['wrong count',s=>s.triangle_count=10,'invalid_data'],
 ['wrong rule',s=>s.triangulation='fan','invalid_data'],
 ['wrong bound',s=>s.polygon_vertex_limit=128,'invalid_data'],
 ['padded face width budget',s=>s.faces=descriptor([2,66],[0,4,1,5,2,6,...Array(126).fill(null)]),'budget_exceeded'],
 ['triangle budget',s=>s.faces.shape[0]=80001,'budget_exceeded'],
 ['interp wrong association',s=>s.face_color.mode='interp','unsupported_patch'],
 ['texture',s=>s.face_color.mode='texturemap','unsupported_patch'],
 ['vertex colours',s=>s.face_color=flat(Array(8).fill(1)),'patch_colors'],
 ['alpha',s=>s.facealpha=.5,'transparency'],
 ['negative width',s=>s.line_width=-1,'invalid_data'],
 ['invalid truecolor',s=>s.face_color=flat([2,0,0],'truecolor'),'unsupported_color'],
 ['invalid integer',s=>s.face_color=flat([.5,2],'indexed','direct','uint8'),'invalid_data']
])test('reject '+name,()=>{const f=figure();mutate(f.axes[0].series[0]);const r=validate(f);assert.equal(r.ok,false);assert.equal(r.reason_code,reason,JSON.stringify(r));});
test('mocked GL uploads patch faces and draws only source edges through existing renderer',()=>{
 const filename=path.resolve('frontend/figure_webgl.js'),m=new Module(filename,module);m.filename=filename;m.paths=Module._nodeModulePaths(path.dirname(filename));
 m._compile(esbuild.transformSync(fs.readFileSync(filename,'utf8'),{format:'cjs',target:'node18'}).code,filename);
 const uploads=[],elements=[],draws=[];let serial=0;
 const gl=new Proxy({createBuffer:()=>++serial,createVertexArray:()=>++serial,createShader:()=>++serial,createProgram:()=>++serial,
 getShaderParameter:()=>true,getProgramParameter:()=>true,getUniformLocation:(_,name)=>name,isContextLost:()=>false,getExtension:()=>null,
 bufferData:(_,v)=>uploads.push(v.slice()),drawElements:(_,count)=>elements.push(count),drawArrays:(_,start,count)=>draws.push(count)},
 {get:(o,k)=>k in o?o[k]:/^[A-Z_]+$/.test(k)?k:()=>{}});
 const f=figure(),scene=geometry.buildScene(f.axes[0]),canvas={width:600,height:400,getContext:()=>gl,addEventListener(){},removeEventListener(){}};
 const renderer=new m.exports.FigureWebGL(canvas,[scene],()=>assert.fail('fallback'));
 assert.deepEqual(uploads[0],scene.meshes[0].positions);assert.deepEqual(uploads[1],scene.meshes[0].colors);
 renderer.render([{scene,camera:camera.createCamera(f.axes[0]),viewport:{x:0,y:0,width:600,height:400}}],{background:[0,0,0],text:[1,1,1],grid:[.1,.1,.1],axis:[.5,.5,.5]},1);
 assert.deepEqual(elements,[12]);assert.deepEqual(draws,[48]);
 for (const style of ['--', ':', '-.']) {
   scene.edges[0].style=style;uploads.length=0;
   const cam=camera.createCamera(f.axes[0]),vp={x:0,y:0,width:600,height:400};
   renderer.render([{scene,camera:cam,viewport:vp}],{background:[0,0,0],text:[1,1,1],grid:[.1,.1,.1],axis:[.5,.5,.5]},1);
   const projected=Array.from({length:scene.edges[0].positions.length/3},(_,i)=>camera.project(cam,vp,scene.edges[0].positions.subarray(i*3,i*3+3)));
   const expected=[];
   for(let face=0;face<2;face++) {
     const i=face*8,path=[projected[i],projected[i+1],projected[i+3],projected[i+5],projected[i+7]];
     expected.push(...m.exports.screenPolyline(path,2*96/72,undefined,style,vp));
   }
   assert.deepEqual(uploads[0],new Float32Array(expected),'dash phase must continue around each source polygon');
 }
 renderer.dispose();
});
console.log(`${cases} STAGE4 integration cases passed (graphics/browser unverified)`);
