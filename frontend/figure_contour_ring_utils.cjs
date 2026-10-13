"use strict";
const {patchPoint}=require("./figure_patch3d_utils.cjs");
const {polygon,createContourWork}=require("./figure_contour_triangulate_utils.cjs");
// Cache only a fully validated record. Every admission recomputes topology.
const admitted = new WeakMap();
function rememberContourGeometry(s, geometry) { admitted.set(s, geometry); }
function contourRingGeometry(s,work=createContourWork()){
 try {
  work.check(true);
  const points=Array.from({length:s.vertices.shape[0]},(_,i)=>{work.check();return patchPoint(s,i);});
  const face=s.faces.values;
  const fail=(code,args)=>({code,args,triangles:[],faceOfTriangle:[],edges:[]});
  if(s.no_area){
    const unique=new Set(face.map(i=>{work.check();return JSON.stringify(points[i]);}));
    if(unique.size>2||unique.size===0)return fail("contour_filled",{property:"faces",detail:"no_area"});
    const edges=face.length>1?face.map((v,i)=>{work.check();return [v,face[(i+1)%face.length],0];}):[];
    return {code:"",args:{},triangles:[],faceOfTriangle:[],edges};
  }
  const triangles=polygon(points,face,work);
  if(!triangles)return fail("contour_filled",{property:"faces",detail:"invalid_geometry"});
  return {code:"",args:{},triangles,faceOfTriangle:triangles.map(()=>{work.check();return 0;}),edges:face.map((v,i)=>{work.check();return [v,face[(i+1)%face.length],0];})};
 } catch(e) {
  if(e.reason_code!=="budget_exceeded")throw e;
  return {code:e.reason_code,args:e.reason_args,triangles:[],faceOfTriangle:[],edges:[]};
 }
}
function patchGeometry(s){
  if(s.triangulation==="native-ring-v1")return admitted.get(s) || contourRingGeometry(s);
  const rows=d=>Array.from({length:d.shape[0]},(_,i)=>Array.from({length:d.shape[1]},(_,k)=>d.values[i+k*d.shape[0]]));
  return {triangles:rows(s.triangle_indices),faceOfTriangle:s.triangle_faces,edges:rows(s.edge_indices)};
}
module.exports={contourRingGeometry,patchGeometry,rememberContourGeometry};
