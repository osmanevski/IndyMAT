#!/bin/sh
# Cross-check: run an .m script on the IndyMAT engine and on octave-cli
# (behaviour oracle only) and diff the numeric output.
# usage: motor/scripts/octave_xc.sh <script.m>
set -e
f=$(cd "$(dirname "$1")" && pwd)/$(basename "$1"); d=$(dirname "$f"); n=$(basename "$f" .m)
bin="$(dirname "$0")/../target/release/indy"
"$bin" run "$f" > "${TMPDIR:-/tmp}/xc_indy_$n.txt" 2>&1 || true
(cd "$d" && octave-cli --no-gui -q --eval "$n" > "${TMPDIR:-/tmp}/xc_oct_$n.txt" 2>&1) || true
python3 -I - "${TMPDIR:-/tmp}/xc_indy_$n.txt" "${TMPDIR:-/tmp}/xc_oct_$n.txt" <<'PY'
import sys,re
a=open(sys.argv[1]).read().splitlines(); b=open(sys.argv[2]).read().splitlines()
bad=0
for i in range(max(len(a),len(b))):
    x=a[i] if i<len(a) else '<missing>'; y=b[i] if i<len(b) else '<missing>'
    if x==y: continue
    ta,tb=x.split('|'),y.split('|')
    if len(ta)==2 and len(tb)==2 and ta[0]==tb[0]:
        try:
            u=[float(t.replace('NA','nan')) for t in ta[1].split()]; v=[float(t.replace('NA','nan')) for t in tb[1].split()]
            if len(u)==len(v) and all((p!=p and q!=q) or p==q or abs(p-q)<=1e-8*max(1,abs(q)) for p,q in zip(u,v)): continue
        except ValueError: pass
    bad+=1; print('DIFF indy:',x); print('     oct :',y)
print('lines',len(a),'differences',bad)
sys.exit(1 if bad else 0)
PY
