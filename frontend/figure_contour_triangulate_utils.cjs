"use strict";
// Large native rings use the same normalized predicates and first-ear order as
// patches, with sparse duplicate lookup, bounding-box rejection and a linked
// ear list. Source vertices and boundary indices never change.
// Count bounded work units (vertex/edge visits, bucket probes, sort comparisons,
// candidate ears and reflex containment tests), including rejected candidates.
// One figure shares this budget across all its native rings. The deadline is
// cooperative: checked every 64 units, not a hard real-time/GC guarantee.
const WORK_LIMITS = Object.freeze({operations:500000, milliseconds:100, clockInterval:64});
function createContourWork({operations=WORK_LIMITS.operations, milliseconds=WORK_LIMITS.milliseconds, clock=()=>performance.now()}={}) {
 const started = clock();
 const fail = (budget, actual, limit) => {
  const error = new Error("contour work limit");
  error.reason_code = "budget_exceeded";
  error.reason_args = {budget, actual, limit};
  throw error;
 };
 return {
  operations: 0,
  check(force=false) {
   if (++this.operations > operations) fail("contour_operations", this.operations, operations);
   if (force || this.operations % WORK_LIMITS.clockInterval === 0) {
    const elapsed = clock() - started;
    if (elapsed > milliseconds) fail("contour_time_ms", Math.ceil(elapsed), milliseconds);
   }
  }
 };
}
const EPS=1e-10;
const cross=(a,b,c)=>(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);
const on=(a,b,p)=>Math.abs(cross(a,b,p))<=EPS&&p[0]>=Math.min(a[0],b[0])-EPS&&p[0]<=Math.max(a[0],b[0])+EPS&&p[1]>=Math.min(a[1],b[1])-EPS&&p[1]<=Math.max(a[1],b[1])+EPS;
const intersects=(a,b,c,d)=>{
 const p=cross(a,b,c),q=cross(a,b,d),r=cross(c,d,a),s=cross(c,d,b);
 return ((p>EPS&&q< -EPS||p< -EPS&&q>EPS)&&(r>EPS&&s< -EPS||r< -EPS&&s>EPS))||on(a,b,c)||on(a,b,d)||on(c,d,a)||on(c,d,b);
};
function polygon(vertices,face,work=createContourWork()){
 const step=()=>work.check();work.check(true);
 const n=face.length;let scale=0;
 for(const i of face){step();for(const v of vertices[i])scale=Math.max(scale,Math.abs(v));}
 if(!(scale>0))return null;
 const origin=vertices[face[0]].map(v=>v/scale);let extent=0;
 let points=face.map(i=>{step();return vertices[i].map((v,k)=>{const x=v/scale-origin[k];extent=Math.max(extent,Math.abs(x));return x;});});
 if(!(extent>0))return null;points=points.map(p=>{step();return p.map(v=>v/extent);});
 const normal=[0,0,0];
 for(let i=0;i<n;i++){step();const a=points[i],b=points[(i+1)%n];normal[0]+=(a[1]-b[1])*(a[2]+b[2]);normal[1]+=(a[2]-b[2])*(a[0]+b[0]);normal[2]+=(a[0]-b[0])*(a[1]+b[1]);}
 const length=Math.hypot(...normal);if(!(length>EPS)||points.some(p=>{step();return Math.abs(p.reduce((a,v,k)=>a+v*normal[k],0))>EPS*length;}))return null;
 let drop=0;for(let k=1;k<3;k++)if(Math.abs(normal[k])>Math.abs(normal[drop]))drop=k;
 const xy=points.map(p=>{step();return p.filter((_,k)=>k!==drop);});
 // Tolerance-equivalent duplicate check, including nonadjacent repeated points.
 const buckets=new Map();
 for(const p of points){
  step();const cell=p.map(v=>Math.floor(v/EPS));
  for(let x=-1;x<=1;x++)for(let y=-1;y<=1;y++)for(let z=-1;z<=1;z++){
   step();const bucket=buckets.get(`${cell[0]+x},${cell[1]+y},${cell[2]+z}`);
   if(bucket?.some(q=>{step();return p.every((v,k)=>Math.abs(v-q[k])<=EPS);}))return null;
  }
  const key=cell.join(',');if(!buckets.has(key))buckets.set(key,[]);buckets.get(key).push(p);
 }
 const edges=xy.map((a,i)=>{step();const b=xy[(i+1)%n];return {i,a,b,x0:Math.min(a[0],b[0]),x1:Math.max(a[0],b[0]),y0:Math.min(a[1],b[1]),y1:Math.max(a[1],b[1])};}).sort((a,b)=>{step();return a.x0-b.x0||a.i-b.i;});
 let active=[];
 for(const edge of edges){
  step();active=active.filter(a=>{step();return a.x1>=edge.x0-EPS;});
  for(const other of active){step();
   if(Math.abs(edge.i-other.i)===1||Math.abs(edge.i-other.i)===n-1)continue;
   if(other.y1<edge.y0-EPS||edge.y1<other.y0-EPS)continue;
   if(intersects(edge.a,edge.b,other.a,other.b))return null;
  }
  active.push(edge);
 }
 let area=0;for(let i=0;i<n;i++){step();area+=xy[i][0]*xy[(i+1)%n][1]-xy[(i+1)%n][0]*xy[i][1];}
 if(Math.abs(area)<=EPS)return null;const winding=Math.sign(area);
 const prev=Int32Array.from({length:n},(_,i)=>{step();return (i+n-1)%n;}),next=Int32Array.from({length:n},(_,i)=>{step();return (i+1)%n;}),reflex=new Set();
 const update=i=>{step();if(winding*cross(xy[prev[i]],xy[i],xy[next[i]])<=EPS)reflex.add(i);else reflex.delete(i);};
 for(let i=0;i<n;i++){step();
  const a=xy[prev[i]],b=xy[i],c=xy[next[i]];
  if(Math.abs(cross(a,b,c))<=EPS&&(a[0]-b[0])*(c[0]-b[0])+(a[1]-b[1])*(c[1]-b[1])>EPS)return null;
  update(i);
 }
 const triangles=[];let start=0,count=n;
 while(count>3){
  let b=start,found=false;
  do{step();
   const a=prev[b],c=next[b];
   if(!reflex.has(b)){
    let blocked=false;
    for(const p of reflex){step();if(p===a||p===b||p===c)continue;
     if(winding*cross(xy[a],xy[b],xy[p])>=-EPS&&winding*cross(xy[b],xy[c],xy[p])>=-EPS&&winding*cross(xy[c],xy[a],xy[p])>=-EPS){blocked=true;break;}}
    if(!blocked){triangles.push([face[a],face[b],face[c]]);next[a]=c;prev[c]=a;reflex.delete(b);if(b===start)start=c;count--;update(a);update(c);found=true;break;}
   }
   b=next[b];
  }while(b!==start);
  if(!found)return null;
 }
 const a=start,b=next[a],c=next[b];if(winding*cross(xy[a],xy[b],xy[c])<=EPS)return null;
 triangles.push([face[a],face[b],face[c]]);return triangles;
}
module.exports={polygon,createContourWork,WORK_LIMITS};
