"use strict";
const assert = require('node:assert/strict');
const fs = require('node:fs');
const {validate} = require('../frontend/figure_data_utils.cjs');
const colors = require('../frontend/figure_color_utils.cjs');
const geometry = require('../frontend/figure_geometry_utils.cjs');
const camera = require('../frontend/figure_camera_utils.cjs');
const picking = require('../frontend/figure_pick_utils.cjs');
const {tipText} = require('../frontend/figure_tool_utils.cjs');
const d = (shape,values) => ({shape,values,order:'column-major'});
function figure(is3=true) {
 const f=JSON.parse(fs.readFileSync('tests/fixtures/figure_v3/surf_vector.json'));
 const a=f.axes[0];a.dimension=is3?3:2;a.xlim=a.ylim=a.zlim=[0,4];a.clim=[1,3];
 a.view=[0,90];Object.assign(a.camera,{position:[2,2,10],target:[2,2,2],up_vector:[0,1,0]});
 a.colormap=d([3,3],[1,0,0,0,1,0,0,0,1]);a.decimated=false;
 const s={id:0,kind:'line',role:'contour',level:2,contour_closed:false,
 x:[1,2,null,3,4],y:[1,2,null,3,4],line_color:[0,1,0],
 marker:'none',marker_size:0,marker_edge_color:'none',marker_face_color:'none',marker_face_auto:false,
 line_width:2,line_style:'--',display_name:'Contour',decimated:false,original_points:5,rendered_points:5,source_indices:[0,1,2,3,4],
 contour_color:{mode:'flat',association:'level',encoding:'indexed',mapping:'scaled',cdata_class:'double',data:d([1,1],[2])}};
 if(is3)s.z=[2,2,null,2,2];a.series=[s];f.vertex_count=5;f.triangle_count=0;f.decimated=false;return f;
}
for(const is3 of [true,false])assert(validate(figure(is3)).ok);
for(const is3 of [true,false]) {
 const closed=figure(is3),s=closed.axes[0].series[0];
 Object.assign(s,{x:[0,1,1,0],y:[0,0,1,0],source_indices:[0,1,2,3],original_points:4,rendered_points:4,contour_closed:true});
 if(is3)s.z=[2,2,2,2];closed.vertex_count=4;
 assert(validate(closed).ok);assert.deepEqual([...geometry.buildScene(closed.axes[0]).lines[0].breaks],[0,4]);
}
const f=figure(),a=f.axes[0],s=a.series[0];const before=structuredClone(f);
const scene=geometry.buildScene(a);assert.deepEqual(f,before);
assert.deepEqual([...scene.lines[0].breaks],[0,2,4]);
assert.deepEqual(scene.lines[0].color,[0,1,0]);
for(const [level,expected] of [[-1,[1,0,0]],[1,[1,0,0]],[2,[0,1,0]],[3,[0,0,1]],[100,[0,0,1]]]) {
 const c=structuredClone(s);c.level=c.contour_color.data.values[0]=level;
 assert.deepEqual(colors.resolveContourColor(c,a),expected);
}
const cam=camera.createCamera(a),vp={width:600,height:400};
const pixel=camera.project(cam,vp,scene.lines[0].positions.subarray(0,3));
const hit=picking.pick(scene,cam,vp,pixel.x,pixel.y);assert(hit);assert.equal(hit.level,2);
assert.deepEqual(hit.data,{x:1,y:1,z:2});assert.equal(hit.index,0);
assert.match(tipText({...hit.data,level:hit.level}),/Level: 2/);
for(const mutate of [s=>delete s.level,s=>s.level=null,s=>s.contour_closed=1,s=>s.contour_closed=true,s=>s.contour_color.data.values[0]=1,s=>s.contour_color.mapping='bad',s=>s.marker='o']) {
 const bad=figure();mutate(bad.axes[0].series[0]);assert.equal(validate(bad).ok,false);
}
for(const code of ['contour_filled','contour_labels']) {
 const bad=figure();Object.assign(bad,{supported:false,axes:[],reason_code:code,reason_args:{}});assert(validate(bad).ok);
}
console.log('CONTOUR UNIT PASS: paths/gaps, LevelList CLim colours, original XYZ/level tips, validator boundaries');
