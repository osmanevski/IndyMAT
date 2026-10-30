#!/usr/bin/env bash
# Behaviour oracle for indy-graphics: runs every case of
# motor/crates/indy-graphics/tests/oracle/cases.txt in a FRESH octave-cli
# process (fltk, invisible figure) through IndyMAT's own extractor
# octave/__mf_figure_data__.m and writes <name>.json (or <name>.error.txt).
#
# usage: grafik_oracle.sh [out_dir] [case_name ...]
#   out_dir defaults to motor/crates/indy-graphics/tests/oracle
#   case names default to all cases in cases.txt
# env: GRAFIK_TOOLKIT=fltk (default) | qt  (qt runs `octave --no-gui` with
#      QT_QPA_PLATFORM=offscreen; only for fltk-vs-qt cross-checks)
#      | qtnative (`octave --no-gui` on the native platform, exactly as the
#      IndyMAT app runs Octave; its text extents are the ones users see)
#      GRAFIK_CASES=<file>  another cases file (out_dir defaults to its dir)
#      GRAFIK_COMPAT=1      also addpath octave/compat (IndyMAT's own MIT
#                           functions such as sgtitle, which Octave lacks)
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cases_file=${GRAFIK_CASES:-"$here/../crates/indy-graphics/tests/oracle/cases.txt"}
cases_file=$(cd "$(dirname "$cases_file")" && pwd)/$(basename "$cases_file")
out_dir=${1:-$(dirname "$cases_file")}
[[ $# -gt 0 ]] && shift
mkdir -p "$out_dir"
out_dir=$(cd "$out_dir" && pwd)

TOOLKIT=${GRAFIK_TOOLKIT:-fltk}
if [[ $TOOLKIT == qt ]]; then
  OCTAVE=${OCTAVE:-octave}; export QT_QPA_PLATFORM=${QT_QPA_PLATFORM:-offscreen}
elif [[ $TOOLKIT == qtnative ]]; then
  OCTAVE=${OCTAVE:-octave}; unset QT_QPA_PLATFORM; TOOLKIT=qt
else
  OCTAVE=${OCTAVE:-octave-cli}
fi
export GRAFIK_TOOLKIT=$TOOLKIT
command -v "$OCTAVE" >/dev/null || { echo "$OCTAVE not found" >&2; exit 127; }

if [[ $# -gt 0 ]]; then
  names=("$@")
else
  names=()
  while IFS= read -r n; do names+=("$n"); done < <(sed -n 's/^[[:space:]]*%%[[:space:]]*\([^[:space:]]*\).*$/\1/p' "$cases_file")
fi

esc() { printf '%s' "$1" | sed "s/'/''/g"; }
nfail=0
for name in "${names[@]}"; do
  # Fresh process per case: no colormap/hold/default state can leak.
  LC_ALL=C "$OCTAVE" --quiet --no-init-file --no-site-file --no-history --no-gui \
    --eval "addpath('$(esc "$here")'); grafik_oracle('$(esc "$cases_file")','$(esc "$out_dir")','$(esc "$name")');" \
    2> >(grep -v -e 'fltk graphics toolkit' -e 'FLTK graphics toolkit' -e 'actively maintained' \
                 -e 'limitations that are unlikely' -e 'qt toolkit is recommended' -e '^$' >&2) \
    || echo "FAIL  $name: octave exited with $?" >&2
  [[ -f "$out_dir/$name.json" ]] || nfail=$((nfail + 1))
done
echo "grafik_oracle: ${#names[@]} cases, $nfail failed ($TOOLKIT) -> $out_dir"
exit 0
