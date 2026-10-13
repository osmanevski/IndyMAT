"use strict";
const assert = require('node:assert/strict');
const S = require('./figure_dalga26_support.cjs');
const {validate}=require('../frontend/figure_data_utils.cjs');
const G=require('../frontend/figure_geometry_utils.cjs');
const C=require('../frontend/figure_camera_utils.cjs');
const P=require('../frontend/figure_pick_utils.cjs');
const alpha=value=>({mode:'constant',association:'constant',value});
for(const make of [S.interpolatedPatch,S.interpolatedSurface]){
 const f=make(),s=f.axes[0].series[0],before=G.buildScene(f.axes[0]);
 assert.equal(before.hasTransparency,undefined);
 for(const object of [...before.meshes,...before.edges])assert.equal(object.alphas,undefined);
 s.face_alpha=alpha(.25);s.edge_alpha=alpha(.75);assert(validate(f).ok);
 const scene=G.buildScene(f.axes[0]);assert.equal(scene.hasTransparency,true);
 for(const [objects,value] of [[scene.meshes,.25],[scene.edges,.75]])for(const object of objects){assert.equal(object.alphas.length,object.positions.length/3);assert(object.alphas.every(v=>v===value));}
 for(const collection of ['meshes','edges']) for(let i=0;i<scene[collection].length;i++)for(const key of ['positions','colors','indices'])assert.deepEqual(scene[collection][i][key],before[collection][i][key]);
 for(const key of ['face_alpha','edge_alpha']){
  const old=s[key];s[key]={mode:'interp',association:'vertex',values:[0,.5,1]};assert.equal(validate(f).reason_code,'transparency');s[key]=old;
 }
 s.face_alpha=alpha(1);s.edge_alpha=alpha(1);const opaque=G.buildScene(f.axes[0]);assert.equal(opaque.hasTransparency,undefined);assert(opaque.meshes.every(m=>!m.alphas));
}
const front=S.patch([[0,0,1],[1,0,1],[0,1,1]],[[0,1,2]]),back=S.patch([[0,0,0],[1,0,0],[0,1,0]],[[0,1,2]],1);
const f=S.figure([front,back]),a=f.axes[0],cam=C.createCamera(a),vp={width:400,height:400};
front.edge_color={mode:'none',association:'constant'};back.edge_color=front.edge_color;
const xy=C.project(cam,vp,C.dataToNormalised(a,[.1,.1,1]));
for(const [value,expected] of [[.5,0],[0,1]]){front.face_alpha=alpha(value);assert(validate(f).ok);assert.equal(P.pick(G.buildScene(a),cam,vp,xy.x,xy.y).series,expected);}
console.log('PASS scalar alpha validator→buffers→picking, unchanged positions/RGB, omitted opaque uploads, data modes rejected');
