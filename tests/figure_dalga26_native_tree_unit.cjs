"use strict";
const assert=require("node:assert/strict");
const measured=require("./fixtures/contourf_measured_dalga26.json");
const {patch,figure,matrix,descriptor}=require("./figure_dalga26_support.cjs");
const {validate}=require("../frontend/figure_data_utils.cjs");
const geometry=require("../frontend/figure_geometry_utils.cjs");
const camera=require("../frontend/figure_camera_utils.cjs");
const {pick}=require("../frontend/figure_pick_utils.cjs");
function record(raw,id){
  const vertices=raw.vertices.rows.map(row=>row.length===2?[...row,0]:row);
  // Canonical indices independently transcribed from measured duplicate runs.
  const canonical=id===0?[0,1,2,4,6,7]:[0,1,3,5];
  const s=patch(vertices,[canonical],id);
  Object.assign(s,{role:"contourf",level:raw.facevertexcdata.rows[0][0],contour_group:0,
    source_faces:matrix(raw.faces.rows.map(row=>row.map(index=>index-1))),
    face_color:{mode:"flat",association:"face",encoding:"indexed",mapping:"scaled",cdata_class:"double",data:descriptor([1,1],[raw.facevertexcdata.rows[0][0]])}});
  return s;
}
for(const caseIndex of [0,2]){
  const native=measured.cases[caseIndex].patches_in_draw_order;
  const f=figure(native.map(record)),axes=f.axes[0];axes.xlim=[1,3];axes.ylim=[1,2];axes.zlim=[-1,8];
  axes.camera.target=[2,1.5,0];axes.camera.position=[2,1.5,20];
  assert(validate(f).ok,JSON.stringify(validate(f)));
  assert.equal(f.vertex_count,14);assert.equal(f.triangle_count,6);
  const scene=geometry.buildScene(axes);assert.equal(scene.meshes.length,2);
  for(let i=0;i<2;i++){
    assert.deepEqual(f.axes[0].series[i].vertices.values,matrix(native[i].vertices.rows.map(row=>row.length===2?[...row,0]:row)).values);
    assert(scene.meshes[i].source.vertexIndices.every(index=>(i===0?[0,1,2,4,6,7]:[0,1,3,5]).includes(index)));
  }
  const vp={x:0,y:0,width:400,height:400},cam=camera.createCamera(axes);
  const z=caseIndex===2?7:0;
  const screen=camera.project(cam,vp,camera.dataToNormalised(axes,[2.99,1.99,z]));
  const tip=pick(scene,cam,vp,screen.x,screen.y);
  assert.equal(tip.level,1.5);assert.equal(tip.data.z,z);
  assert(native[1].vertices.rows.some(row=>row[0]===tip.data.x&&row[1]===tip.data.y));
  for(const mutate of [s=>s.source_faces.values[2]=99,s=>s.source_faces.values[2]=null,
    s=>s.source_faces.values.splice(3,1,0),s=>s.source_faces.shape=[2,4]]){
    const bad=structuredClone(f);mutate(bad.axes[0].series[0]);assert(!validate(bad).ok);
  }
  // Retaining duplicates in the rendered face would recreate the real failure.
  const bad=structuredClone(f);bad.axes[0].series[0].faces=structuredClone(bad.axes[0].series[0].source_faces);
  assert(!validate(bad).ok);
}
console.log("PASS native contour tree replay: duplicate connectivity, original XY/Z and level tips, exact raw-face validation");
