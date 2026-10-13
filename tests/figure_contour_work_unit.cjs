"use strict";
const assert = require("node:assert/strict");
const { native, nativeFigure } = require("./figure_contour_round3_support.cjs");
const { validate } = require("../frontend/figure_data_utils.cjs");
const ring = n => native({ vertices: [[0,0],[n,0],[n,2], ...Array.from({length:n},(_,k)=>{const i=n-1-k;return [i,i%2?2:1];})],
  faces: Array.from({length:n+3},(_,i)=>i+1), zdata: [], facevertexcdata: 1, facecolor: [1,0,0], edgecolor: "none" });
const sawtooth = nativeFigure([ring(32000)]);
const result = validate(sawtooth);
assert.equal(result.reason_code, "budget_exceeded", "valid 32,003-corner sawtooth must not monopolize validation");
assert(["contour_operations", "contour_time_ms"].includes(result.reason_args.budget));
const { createContourWork, WORK_LIMITS } = require("../frontend/figure_contour_triangulate_utils.cjs");
const { contourRingGeometry } = require("../frontend/figure_contour_ring_utils.cjs");
const work = createContourWork({clock:()=>0});
const limited = contourRingGeometry(sawtooth.axes[0].series[0], work);
assert.equal(limited.code, "budget_exceeded");
assert.deepEqual(limited.args, {budget:"contour_operations",actual:WORK_LIMITS.operations+1,limit:WORK_LIMITS.operations});
assert.equal(work.operations, WORK_LIMITS.operations+1, "operation bound, independent of machine speed");
// This smaller ring fits preprocessing comfortably but exhausts ear testing.
const earWork=createContourWork({clock:()=>0});
assert.equal(contourRingGeometry(ring(2000),earWork).args.budget,"contour_operations");
assert.equal(earWork.operations,WORK_LIMITS.operations+1);
let now=0;
const timed = createContourWork({clock:()=>now});
now=WORK_LIMITS.milliseconds+1;
assert.equal(contourRingGeometry(ring(8),timed).args.budget,"contour_time_ms");
// Budget is shared across children, not reset for each ring.
const shared = createContourWork({operations:1, clock:()=>0});
assert.equal(contourRingGeometry(ring(8),shared).code,"budget_exceeded");
console.log("PASS sawtooth predicate/work cap, injected deadline, shared contour budget");
const { createHash } = require("node:crypto");
const G = require("../frontend/figure_geometry_utils.cjs");
const captures = require("./fixtures/contour_work_round6.json");
function bufferHash(scene) {
  const h=createHash("sha256");
  function visit(o) {
    if(ArrayBuffer.isView(o)){h.update(Buffer.from(o.buffer,o.byteOffset,o.byteLength));return;}
    if(o&&typeof o==="object")for(const [k,v] of Object.entries(o))if(k!=="source"&&k!=="axes")visit(v);
  }
  visit(scene);return h.digest("hex");
}
for(const c of captures.cases) {
  const records=c.records || require("./fixtures/contourf_round3_native.json").cases[0].record.patches_in_draw_order;
  const rings=records.map(native), f=nativeFigure(rings), work=createContourWork({clock:()=>0});
  for(const ring of rings)assert.equal(contourRingGeometry(ring,work).code,"");
  assert(work.operations<WORK_LIMITS.operations, `peaks${c.n} work remains admitted`);
  const checked=validate(f);assert(checked.ok,JSON.stringify(checked));
  assert.equal(bufferHash(G.buildScene(f.axes[0])),c.bufferHash,`peaks${c.n}: byte-identical buffers to 2027ae3`);
  console.log(`PASS peaks${c.n}: ${work.operations} units; unchanged buffers (${c.n===20?"native coordinates":"numeric replay"})`);
}
// Many individually small rings must consume one shared figure allowance.
const records=captures.cases.find(c=>c.n===100).records;
const accumulated=createContourWork({clock:()=>0});
let exhaustion;
for(const r of [...records,...records].map(native)){
  const g=contourRingGeometry(r,accumulated);if(g.code){exhaustion=g;break;}
}
assert.equal(exhaustion?.args.budget,"contour_operations");
assert.equal(accumulated.operations,WORK_LIMITS.operations+1);
