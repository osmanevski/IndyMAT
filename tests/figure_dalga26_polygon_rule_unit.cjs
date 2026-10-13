"use strict";
const assert = require("node:assert/strict"), fs = require("node:fs");
const {patch, figure, descriptor} = require("./figure_dalga26_support.cjs");
const {validate} = require("../frontend/figure_data_utils.cjs");
const vertices = [[0,0,1],[1,0,1],[1,1,1],[0,1,1]];
const interp = n => ({mode:"interp",association:"vertex",encoding:"indexed",mapping:"scaled",cdata_class:"double",data:descriptor([n,1],Array.from({length:n},(_,i)=>i%2))});
for (const faces of [[[0,1,2,3]],[[3,2,1,0]],[[1,2,3,0]],[[2,3,0,1]],[[0,1,2,3,0]]]) {
  const s=patch(vertices,faces);s.face_color=interp(4);
  const result=validate(figure([s]));
  assert.equal(result.reason_code,"interpolated_color");
  assert.equal(result.reason_args.detail,"polygon_tessellation");
  // Boundary edges have no hidden tessellation choice.
  s.face_color={mode:"none",association:"constant"};s.edge_color=interp(4);
  assert(validate(figure([s])).ok);
}
const pentagon=patch([[0,0,1],[1,0,1],[.5,.4,1],[1,1,1],[0,1,1]],[[0,1,2,3,4]]);
pentagon.face_color=interp(5);assert.equal(validate(figure([pentagon])).reason_code,"interpolated_color");
for (const faces of [[[0,1,2]],[[0,1,2,0]],[[0,1,2],[0,2,3]]]) {
  const s=patch(vertices,faces);s.face_color=interp(4);assert(validate(figure([s])).ok);
}
// Octave 11.3 native PNG observations, olcum-3d-quad.txt lines 2,11,15,24,
// 59. These falsify first-valid-ear-v1 for interpolated quadrilaterals.
const observed={ccwBottomLeft:.004,cwTopLeft:1,truecolorCentre:[128,1,127]};
assert(Math.abs(observed.ccwBottomLeft-Math.abs(.05-.05))<.01);
assert(Math.abs(observed.cwTopLeft-(1-Math.abs(.05+.95-1)))<.01);
const ccw=patch(vertices,[[0,1,2,3]]);
assert.deepEqual(ccw.triangle_indices.values,[3,1,0,2,1,3]);
// The current diagonal 2--4 gives yellow/green at centre, whereas the native
// 1--3 diagonal gives red/blue. Keep this evidence distinct from browser QA.
assert(Math.abs(observed.truecolorCentre[0]-127.5)<2);
assert(observed.truecolorCentre[1]<2);
assert(Math.abs(observed.truecolorCentre[2]-127.5)<2);
// Pentagon colours [0,1,.5,1,0] equal X. Every triangulation preserves this
// affine function, so those samples cannot identify a tessellation rule.
const pv=[[0,0],[1,0],[.5,.4],[1,1],[0,1]],pc=[0,1,.5,1,0];
for(const triangle of [[0,1,2],[0,2,4],[2,3,4],[0,2,3]]){
  const weights=[.2,.3,.5];
  assert.equal(weights.reduce((v,w,i)=>v+w*pc[triangle[i]],0),weights.reduce((v,w,i)=>v+w*pv[triangle[i]][0],0));
}
const source=fs.readFileSync("frontend/figure_tools.js","utf8");
const reason=new Function("t",source.slice(source.indexOf("export function figureReasonText"),source.indexOf("export function figureReductionText")).replace("export function","return function"))(x=>x);
assert.match(reason("interpolated_color",{detail:"polygon_tessellation"}),/more than three corners/);
console.log("PASS measured polygon interpolation boundary: triangles only; polygon edge interpolation retained");
