"use strict";
// Frozen upload bytes from e554827, before contour renderer integration.
const assert=require("node:assert/strict"),fs=require("node:fs"),path=require("node:path");
const Module=require("node:module"),esbuild=require("esbuild"),{createHash}=require("node:crypto");
const geometry=require("../frontend/figure_geometry_utils.cjs"),camera=require("../frontend/figure_camera_utils.cjs");
function snapshots(source) {
 const filename=path.resolve("frontend/figure_webgl.js"),m=new Module(filename,module);
 m.filename=filename;
 m.paths=Module._nodeModulePaths(path.dirname(filename));
 m._compile(esbuild.transformSync(source,{format:"cjs"}).code,filename);
 const result={};
 for(const name of ["surf_vector","mesh_default","scatter3_sizes","plot3_gap","interp"])for(const dark of [true,false]) {
  const a=JSON.parse(fs.readFileSync("tests/fixtures/figure_v3/"+(name==="interp"?"surf_vector":name)+".json")).axes[0];
  if(name==="interp"){const s=a.series[0];s.face_color={mode:"interp",association:"vertex",encoding:"indexed",mapping:s.cdata_mapping,cdata_class:s.cdata_class,data:structuredClone(s.cdata)};}
  const hash=createHash("sha256");let serial=0;
  const gl=new Proxy({getShaderParameter:()=>true,getProgramParameter:()=>true,isContextLost:()=>false,
   bufferData:(_,v)=>{if(ArrayBuffer.isView(v))hash.update(Buffer.from(v.buffer,v.byteOffset,v.byteLength));},
   uniform3fv:(_,v)=>hash.update(Buffer.from(new Float32Array(v).buffer))},
   {get:(o,k)=>o[k]??(k.startsWith("create")?()=>++serial:/^[A-Z_]+$/.test(k)?k:()=>{})});
  const scene=geometry.buildScene(a),renderer=new m.exports.FigureWebGL({width:800,height:600,getContext:()=>gl,addEventListener(){},removeEventListener(){}},[scene],()=>assert.fail());
  const palette={background:(dark?[5,7,25]:[219,232,243]).map(v=>v/255),text:(dark?[224,224,224]:[17,26,50]).map(v=>v/255),axis:[.2,.3,.4],grid:[.3,.4,.5]};
  renderer.render([{scene,camera:camera.createCamera(a),viewport:{x:0,y:0,width:400,height:300}}],palette,2);
  renderer.dispose();result[name+(dark?"-dark":"-light")]=hash.digest("hex");
 }
 return result;
}
module.exports={snapshots};
if(require.main===module){
 assert.deepEqual(snapshots(fs.readFileSync("frontend/figure_webgl.js","utf8")),require("./fixtures/contour_unchanged_buffers.json"));
 console.log("PASS 10 byte-identical non-contour flat/interp/mesh/marker/line GPU upload snapshots");
}
