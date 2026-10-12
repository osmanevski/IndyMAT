"use strict";
const assert = require("node:assert/strict");
const { patchPoint, triangulatePatch, MAX_FACE_VERTICES } = require("../frontend/figure_patch3d_utils.cjs");
const l = [[0,0,0],[3,0,0],[3,1,0],[1,1,0],[1,3,0],[0,3,0]];
const square = [[0,0,0],[2,0,0],[2,2,0],[0,2,0]];
const ring = (n) => Array.from({length:n}, (_,i) => [Math.cos(2*Math.PI*i/n),Math.sin(2*Math.PI*i/n),0]);
const cases = [
  {name:"triangle",vertices:square.slice(0,3),faces:[[0,1,2]]},
  {name:"triangle reversed",vertices:square.slice(0,3),faces:[[2,1,0]]},
  {name:"quad",vertices:square,faces:[[0,1,2,3]]},
  {name:"quad reversed",vertices:square,faces:[[3,2,1,0]]},
  {name:"concave L",vertices:l,faces:[[0,1,2,3,4,5]]},
  {name:"L reversed",vertices:l,faces:[[5,4,3,2,1,0]]},
  {name:"tilted L",vertices:l.map(([x,y])=>[x,y,x+2*y]),faces:[[0,1,2,3,4,5]]},
  {name:"explicit closure",vertices:square,faces:[[0,1,2,3,0]]},
  {name:"multiple faces",vertices:square,faces:[[0,1,2],[0,2,3]]},
  {name:"collinear boundary",vertices:[[0,0,0],[1,0,0],[2,0,0],[2,2,0],[0,2,0]],faces:[[0,1,2,3,4]]},
  {name:"bowtie",vertices:square,faces:[[0,2,1,3]],code:"unsupported_patch"},
  {name:"nonplanar",vertices:[[0,0,0],[2,0,0],[2,2,1],[0,2,0]],faces:[[0,1,2,3]],code:"unsupported_patch"},
  {name:"duplicate index",vertices:square,faces:[[0,1,2,1]],code:"unsupported_patch"},
  {name:"duplicate position",vertices:[...square,[2,0,0]],faces:[[0,1,4,2,3]],code:"unsupported_patch"},
  {name:"zero area",vertices:[[0,0,0],[1,0,0],[2,0,0]],faces:[[0,1,2]],code:"unsupported_patch"},
  {name:"backtracking",vertices:[[0,0,0],[2,0,0],[1,0,0],[2,2,0],[0,2,0]],faces:[[0,1,2,3,4]],code:"unsupported_patch"},
  {name:"touching nonneighbor",vertices:[[0,0,0],[2,0,0],[2,2,0],[1,0,0],[0,2,0]],faces:[[0,1,2,3,4]],code:"unsupported_patch"},
  {name:"maximum polygon",vertices:ring(64),faces:[Array.from({length:64},(_,i)=>i)]},
  {name:"65 vertices",vertices:ring(65),faces:[Array.from({length:65},(_,i)=>i)],code:"budget_exceeded"},
  {name:"invalid negative index",vertices:square,faces:[[-1,1,2]],code:"invalid_index"},
  {name:"invalid upper index",vertices:square,faces:[[0,1,4]],code:"invalid_index"},
  {name:"invalid fractional index",vertices:square,faces:[[0,1,2.5]],code:"invalid_index"},
  {name:"short face",vertices:square,faces:[[0,1]],code:"invalid_data"},
  {name:"empty",vertices:[],faces:[]},
  {name:"tiny square",vertices:square.map(p=>p.map(v=>v*1e-200)),faces:[[0,1,2,3]]},
  {name:"huge square",vertices:square.map(p=>p.map(v=>v*1e200)),faces:[[0,1,2,3]]},
];
function run() {
  assert.equal(MAX_FACE_VERTICES,64);
  assert.deepEqual(patchPoint({vertices:{shape:[2,3],order:"column-major",values:[1,2,3,4,5,6]}},1),[2,4,6]);
  for (const c of cases) {
    const before = JSON.stringify(c), result = triangulatePatch(c.vertices,c.faces);
    assert.equal(result.code,c.code || "",c.name);
    assert.equal(JSON.stringify(c),before,"pure: "+c.name);
    assert.deepEqual(result,triangulatePatch(c.vertices,c.faces),"deterministic: "+c.name);
    if (result.code) {
      assert.deepEqual([result.triangles,result.faceOfTriangle,result.edges],[[],[],[]]);
      continue;
    }
    let triangleCount = 0;
    c.faces.forEach((raw,f) => {
      const face = raw.length>3 && raw[0]===raw.at(-1) ? raw.slice(0,-1):raw;
      triangleCount += face.length-2;
      assert.deepEqual(result.edges.filter(e=>e[2]===f),face.map((v,i)=>[v,face[(i+1)%face.length],f]),"source edges: "+c.name);
      const tris = result.triangles.filter((_,i)=>result.faceOfTriangle[i]===f);
      assert.equal(tris.length,face.length-2);
      for(const t of tris) assert(t.every(v=>face.includes(v)));
    });
    assert.equal(result.triangles.length,triangleCount);
  }
  const result = triangulatePatch(l,[[0,1,2,3,4,5]]);
  assert.deepEqual(result.triangles,[[0,1,2],[0,2,3],[5,0,3],[3,4,5]]);
  const triangleArea = (t) => { const [a,b,c]=t.map(i=>l[i]);return ((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]))/2; };
  assert.equal(result.triangles.reduce((sum,t)=>sum+triangleArea(t),0),5);
  assert(result.triangles.every(t=>triangleArea(t)>0));
  // Rejection is atomic, including a valid face before an invalid one.
  assert.deepEqual(triangulatePatch(square,[[0,1,2],[0,2,1,3]]).triangles,[]);
  assert.equal(triangulatePatch(Array(40001),[]).args.budget,"patch_vertices");
  assert.equal(triangulatePatch(square,Array(40001).fill([0,1,2,3])).args.budget,"patch_triangles");
  assert.equal(triangulatePatch([[0,0,0],[1,0,0],[0,Infinity,0]],[[0,1,2]]).code,"invalid_data");
  assert.equal(triangulatePatch([[0,0,0],[1,0,0],[0,null,0]],[[0,1,2]]).code,"invalid_data");
  assert.equal(triangulatePatch(Array(3),[[0,1,2]]).code,"invalid_data");
  assert.equal(triangulatePatch([[0,0,0],[1,0,0],Array(3)],[[0,1,2]]).code,"invalid_data");
  assert.equal(triangulatePatch(square,[Array(3)]).code,"invalid_index");
  console.log("patch3d pure geometry: 26 cases, source edges, deterministic L area, bounds passed");
}
if (require.main === module) run();
module.exports = { cases };
