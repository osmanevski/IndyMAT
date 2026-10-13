"use strict";
const assert = require('node:assert/strict');
const fs = require('node:fs');
const source = fs.readFileSync('octave/__mf_figure_data__.m', 'utf8');
// A source regression guard, not a graphics measurement. The companion .m
// acceptance fixture exercises real object visibility on the orchestrator.
const axes = source.slice(source.indexOf('function [a,code,args]=__mf_axes__'), source.indexOf('function [leaves,code,args]=__mf_leaves__'));
assert.match(axes, /for leaf=leaves\s+h=leaf.h;\s+if strcmp\(__mf_prop__\(h,'type',''\),'surface'\)\|\|~isempty\(__mf_prop__\(h,'zdata',\[\]\)\)\|\|strcmp\(leaf.role,'contourf'\),a.dimension=3/);
assert.doesNotMatch(axes, /for h=kids\s+if strcmp\(__mf_prop__\(h,'type',''\),'surface'\)/);
assert.match(source, /strcmp\(__mf_prop__\(h,'visible','on'\),'off'\).*continue/);
console.log('FIGURE VISIBILITY SOURCE GUARD PASS (real graphics unverified)');
