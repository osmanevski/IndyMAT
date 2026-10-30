#!/bin/sh
# Sparse matrices in MAT files, both ways with Octave (behaviour oracle only).
#   1. octave-cli writes -v7, -v6 and -v4; the Rust engine loads and compares;
#   2. the Rust engine writes -v7, -v6 and -v4; octave-cli loads and compares.
# Level 4 has no logical class, cells or structs, so its files hold only the
# numeric variables and logical ones come back as double.
# usage: motor/scripts/matfile_sparse_xc.sh [--update-fixtures]
set -e
here=$(cd "$(dirname "$0")" && pwd)
xc="$here/../crates/indy-interp/tests/matfile_xc"
bin="$here/../target/release/indy"
work="${TMPDIR:-/tmp}/indy_sparse_xc.$$"; mkdir -p "$work"
cp "$xc/sparse_vals.m" "$xc/sparse_check.m" "$xc/sparse_same.m" "$xc/sparse_check4.m" "$work/"
cd "$work"
v4='sp_real sp_inf sp_complex sp_empty sp_00 sp_03 sp_30 sp_1xn sp_nx1 sp_scalar sp_zero sp_full sp_large sp_largerow sp_large_c'
save4="save('-v4', 'XX.mat', 'sp_real','sp_inf','sp_complex','sp_empty','sp_00','sp_03','sp_30','sp_1xn','sp_nx1','sp_scalar','sp_zero','sp_full','sp_large','sp_largerow','sp_large_c')"
octave-cli -q --eval "sparse_vals; save('-v7','oct_sp_v7.mat','sp_*'); save('-v6','oct_sp_v6.mat','sp_*'); $(echo "$save4" | sed s/XX/oct_sp_v4/)"
echo "--- the Rust engine loads Octave's files"
"$bin" eval "files = {'oct_sp_v7.mat','oct_sp_v6.mat'}; sparse_check; files = {'oct_sp_v4.mat'}; sparse_check4"
"$bin" eval "sparse_vals; save indy_sp_v7.mat sp_*; save -v6 indy_sp_v6.mat sp_*; save -v4 indy_sp_v4.mat $v4"
echo "--- Octave loads the Rust engine's files"
octave-cli -q --eval "files = {'indy_sp_v7.mat','indy_sp_v6.mat'}; sparse_check; files = {'indy_sp_v4.mat'}; sparse_check4"
if [ "$1" = "--update-fixtures" ]; then
  # Without the large variables: -v6 stores their column tables uncompressed.
  octave-cli -q --eval "sparse_vals; clear sp_large sp_largerow sp_large_c; save('-v6','oct_sp_small_v6.mat','sp_*')"
  cp oct_sp_v7.mat oct_sp_v4.mat oct_sp_small_v6.mat "$xc/"
fi
rm -rf "$work"
