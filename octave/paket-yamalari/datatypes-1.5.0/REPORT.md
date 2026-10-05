# Wave 7 lane D3 report

## Result

The worktree's private `.packages/datatypes-1.5.0/string.m` is patched. The nine
recorded string failures now pass. Users can pass nonmissing string patterns,
replacement text or delimiters to char/cellstr operations while keeping the
expected result types; use scalar string data with a char compose format; find
empty patterns in string arrays; count non-overlapping matches; concatenate
strings with char text or the documented small-integer numeric subset.

No kernel, compat helper, relocation script, baseline, MATLAB reference record,
or inventory document was changed. No git state-changing command was run.
Application of these patches to the user's real installation remains the
orchestrator's decision.

## Why a package change is needed

Real Octave 11.3.0 (`octave-cli`) was used throughout. Before patching, with the
project helpers on the path, these results were reproduced:

| Call | Original result |
| --- | --- |
| `contains('abc', string('b'))` | `sq_string cannot be indexed with .` |
| `replace('abc', string('b'), 'X')` | `sq_string cannot be indexed with .` |
| `compose('%s', string('a'))` | `string.compose: FORMATSPEC must be a string scalar or a character vector.` |
| `erase('abc', string('b'))` | `sq_string cannot be indexed with .` |
| `split('a,b', string(','))` | `'split_core' undefined` |
| `join({'a','b'}, string('-'))` | `cell cannot be indexed with .` |
| `contains(string('abc'), '')` | false |
| `count(string('aaaa'), 'aa')` | 3 |
| `string('bir') + 'x'`, reverse char order, string + 3 | `string.plus: both STR1 and STR2 must be string arrays.` |

An isolated dispatch probe put a temporary `contains.m` first on the path that
returned `PROJECT_HELPER_CALLED`. `which('contains')` identified it; both direct
and function-handle char-only calls returned that marker. Both direct and
function-handle calls with a later string still entered the original class
method and failed with `sq_string cannot be indexed with .`. Changing helper
precedence or binding `@contains` therefore cannot repair ordinary mixed calls.
Explicit caller-side conversion is a workaround, not a fix to those calls.

## Files added

- `scripts/patch_packages.py`: standard-library hash-guarded apply/check/revert,
  exact unified hunks, verified local backups, atomic target replacement and
  rollback on write errors, package-boundary/symlink checks.
- `tests/test_package_patches.py`: 14 tests using temporary package copies only.
- `uyumluluk/durumlar/42-dizgi-paket.m`: 22 new `dizgip-` cases, valid MATLAB/Octave
  syntax; all named functions appear in their case bodies.
- `octave/paket-yamalari/datatypes-1.5.0/manifest.json`: pinned original/final hashes
  and ordered patch list.
- `01-mixed-dispatch.patch`, `02-contains-empty.patch`,
  `03-count-nonoverlapping.patch`, `04-plus-coercion.patch` in the same directory:
  one topic per diff, one installed target (`string.m`).
- `README.md` and this `REPORT.md` in the same directory: behaviour, restrictions,
  operational instructions, upgrade/reinstall handling and verification.

The untracked private package copy additionally contains the patched target and
its `.indymat-patches` registry/pristine backup. These are not added to Git.
The README describes each patch's user-visible effect and explicit errors.

## Verification

The sandbox has no graphics toolkit and cannot start MATLAB. Case commands used
`MATLAB_FREE_OCTAVE=/opt/homebrew/bin/octave-cli` so the runner stayed in the
permitted CLI engine.

- Before: existing `--cases dizgi`: 56/65 pass, the nine failures above.
- After: `python3 scripts/compat.py --cases dizgi`: 87/87 pass, including all 22
  new cases. `--cases dizgip`: 22/22 pass.
- `python3 -m unittest tests.test_package_patches -v`: 14/14 pass. Tests cover
  byte-identical revert, repeated apply/revert, already-patched skip without a
  backup (revert refused), check without writes, unknown
  target/backup/registry refusal, symlink escapes, missing version, all-target
  preflight, exact diff matching, transaction write-failure rollback, explicit
  errors for unsupported mixed operands, and original-versus-patched string-first
  outputs in separate Octave processes.
  The last comparison covers result types/shapes, ordinary queries/editing,
  compose/split/join, string/string plus, missing source elements, and missing
  patterns. It caught and prevented an unintended missing-pattern change in the
  empty-pattern fix.
- Selected real kernel tests passed: `test_01_persistent`,
  `test_02_error_recovery`, `test_04_inspect`, `test_05_interrupt`,
  `test_06_input`, `test_14_packages` (6/6). They verify persistent values/function
  handles, recovery, inspection, interrupt/input handling and actual
  control/signal/datatypes use. No new standalone function shadows a built-in;
  only the seven existing string methods and string plus are amended.
- Full `python3 scripts/compat.py --check`: exit 1; 247/338 pass. It reports the
  nine existing improvements, 22 new cases missing from the unchanged baseline,
  and 60 previously passing graphics/UI cases failing with exactly
  `no graphics toolkits are available!`.
- Full inventory before/after under the same CLI environment: pristine 219/338,
  patched 247/338, **zero regressions**. The 28 gains are the nine existing fixes
  and 19 new cases that failed with the old package; three new preservation
  cases already passed. Thus the graphics failures are present with pristine
  content too. This does not replace the orchestrator's graphics gate.

`--check` reports the installed target as `patched`; a second apply skips it.
All apply/revert/check tests operate on temporary copies, never the live package.

## Needs MATLAB measurement

The orchestrator must run the reference for all 22 new cases and update its
baseline/reference only after reviewing results. This lane has not independently
verified them on MATLAB. In particular, measure:

- `plus` implicit numeric conversion (the implemented finite real integer scalar
  subset, integer classes, both operand orders); fractional/large values, numeric
  arrays, NaN/Inf, complex/logical/cellstr operands remain unsupported by this
  patch rather than guessed.
- Mixed compose with non-scalar string data, cellstr format + data, empty data,
  and missing operands before extending the existing helper.
- Missing later string patterns/replacements/delimiters/data; the mixed bridge
  currently raises a clear function-specific error.
- String-first count with overlapping different patterns, duplicate patterns and
  empty patterns; existing per-pattern accumulation/empty behaviour is retained.

Known operational limits: restart/reset must be explicit to reload a class in an
already running session; patch application does not do it. A changed package or
upgrade is refused by version/hash checks, never fuzz-patched. Only a
byte-identical reinstall of the pinned original is safe to reapply. The patch
set deliberately does not make claims about complete string-class compatibility.
