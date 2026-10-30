#!/usr/bin/env bash
# Black-box measurements of Octave (fltk) automatic graphics behaviour.
# usage: grafik_measure.sh [out_dir]   -> <out_dir>/measure.json
#   out_dir defaults to motor/crates/indy-graphics/tests/oracle
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
out_dir=${1:-$here/../crates/indy-graphics/tests/oracle}
mkdir -p "$out_dir"
out_dir=$(cd "$out_dir" && pwd)
OCTAVE=${OCTAVE:-octave-cli}
command -v "$OCTAVE" >/dev/null || { echo "octave-cli not found" >&2; exit 127; }
esc() { printf '%s' "$1" | sed "s/'/''/g"; }
LC_ALL=C "$OCTAVE" --quiet --no-init-file --no-site-file --no-history --no-gui \
  --eval "addpath('$(esc "$here")'); grafik_measure('$(esc "$out_dir/measure.json")');" \
  2> >(grep -v -e 'fltk graphics toolkit' -e 'FLTK graphics toolkit' -e 'actively maintained' \
               -e 'limitations that are unlikely' -e 'qt toolkit is recommended' -e '^$' >&2)
