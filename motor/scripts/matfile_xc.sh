#!/bin/sh
# MAT-file interop with Octave (behaviour oracle only):
#   1. octave-cli writes vals.m with -v7 and -v6; the Rust engine loads and compares;
#   2. the Rust engine writes -v7 and -v6; octave-cli loads and compares.
# Octave's own -v6 file holding a multi-row char (`cm`) cannot be loaded by
# Octave itself (the array's size is written too large); the Rust engine reads it.
# usage: motor/scripts/matfile_xc.sh [--update-fixtures]
set -e
here=$(cd "$(dirname "$0")" && pwd)
xc="$here/../crates/indy-interp/tests/matfile_xc"
bin="$here/../target/release/indy"
work="${TMPDIR:-/tmp}/indy_matfile_xc.$$"; mkdir -p "$work"; cp "$xc/vals.m" "$xc/xc_check.m" "$work/"
cd "$work"
octave-cli -q --eval "vals; save('-v7','oct_v7.mat'); save('-v6','oct_v6.mat')"
octave-cli -q --eval "x = {['ab';'cd'], struct('q',['ef';'gh'])}; y = 'zz'; save('-v6','oct_quirk_v6.mat','x','y'); save('-v7','oct_quirk_v7.mat','x','y')"
echo "--- the Rust engine loads Octave's files"
"$bin" eval "files = {'oct_v7.mat','oct_v6.mat'}; xc_check"
"$bin" eval "vals; save indy_v7.mat; save -v6 indy_v6.mat"
echo "--- Octave loads the Rust engine's files"
octave-cli -q --eval "files = {'indy_v7.mat','indy_v6.mat'}; xc_check"
if [ "$1" = "--update-fixtures" ]; then cp oct_v7.mat oct_v6.mat oct_quirk_v6.mat oct_quirk_v7.mat "$xc/"; fi
rm -rf "$work"
