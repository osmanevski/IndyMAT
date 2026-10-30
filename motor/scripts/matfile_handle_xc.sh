#!/bin/sh
# Function handles in MAT files, against what can be checked without MATLAB:
#   1. octave-cli loads the Rust engine's files and calls the handles (Octave cannot
#      write handles to MAT files at all, nor load string arrays from them);
#   2. the Rust engine loads the MATLAB R2025b files in Octave's test data, when
#      installed, and writes the same object bytes (tests/matfile.rs checks it).
# usage: motor/scripts/matfile_handle_xc.sh
set -e
here=$(cd "$(dirname "$0")" && pwd)
bin="$here/../target/release/indy"
work="${TMPDIR:-/tmp}/indy_handle_xc.$$"; mkdir -p "$work"
cd "$work"
"$bin" eval "a = 3; b = [1 2; 3 4]; n = @sin; f = @(x) x + a; g = @(x) b(x, 2) * a; p = @(x) x .^ 2; \
  c = {1, @cos, @(y) y * a}; s = struct('h', @(z) z - a, 'k', 5); \
  save('h7.mat', 'n', 'f', 'g', 'p', 'c', 's'); save('h6.mat', '-v6', 'n', 'f', 'g', 'p', 'c', 's');"
echo "--- Octave loads the Rust engine's files"
octave-cli -q --eval "
for file = {'h7.mat', 'h6.mat'}
  S = load(file{1});
  got = [S.n(0), S.f(2), S.g(2), S.p(3), S.c{1}, S.c{2}(0), S.c{3}(2), S.s.h(5), S.s.k];
  want = [0, 5, 12, 9, 1, 1, 6, 2, 5];
  if ! isequal(got, want), disp(got); error('%s: mismatch', file{1}); end
  printf('%s: same\n', file{1});
end" 2>&1 | grep -v -E "function_handle_workspace|called from|^ +at |^$|top-level|load at"
sample=$(ls /opt/homebrew/Cellar/octave/*/share/octave/*/etc/tests/fixed/bug-67964/foo2.mat 2>/dev/null | head -1 || true)
if [ -n "$sample" ]; then
  echo "--- the Rust engine loads MATLAB R2025b's files"
  "$bin" eval "S = load('$sample'); disp(func2str(S.g)); disp(S.g(2)); S = load('$(dirname "$sample")/foo1.mat'); disp(func2str(S.f)); disp(S.f(2))"
fi
rm -rf "$work"
