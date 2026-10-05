# datatypes 1.5.0 — IndyMAT string patches

These small GPL-3.0-or-later patches modify the package's `string.m`, not Octave
core or the kernel. They require the exact datatypes 1.5.0 file supplied in the
wave 7 private package copy. `manifest.json` pins both its original SHA-256 and
the final patched SHA-256. The original hash is an identity of that supplied
file, not a claim that every installation named 1.5.0 has identical bytes.

| Patch | User-visible behaviour restored | Package behaviour replaced |
| --- | --- | --- |
| `01-mixed-dispatch.patch` | Char/cellstr first inputs accept later nonmissing string operands in `contains`, `count`, `replace`, `erase`, `compose`, `split`, and `join`. Results retain the MATLAB R2025b types: queries return logical/numeric arrays; replacement/deletion retain char or cellstr; compose/split/join return cellstr. | Methods assumed the first input was a string and accessed its private fields, or rejected the char format. |
| `02-contains-empty.patch` | `contains(string('abc'), '')` and `contains(string(''), '')` return true, also with IgnoreCase. Missing source elements remain false; missing string patterns retain their prior nonmatching behaviour. | `strfind` returned no positions for an empty pattern, producing false. |
| `03-count-nonoverlapping.patch` | Single-pattern literal matches are non-overlapping: `count(string('aaaa'), 'aa')` is 2; `count(string('banana'), 'ana')` is 1. | The method counted every `strfind` position, including overlaps. |
| `04-plus-coercion.patch` | String + char and char + string concatenate, preserving spaces; string arrays retain scalar expansion. Numeric concatenation is limited to finite real integer numeric scalars from -9999 through 9999, e.g. `string('x') + 3` is `string('x3')`. | `plus` rejected every operand that was not already a string. |

The existing mixed-argument, empty-pattern and count cases were already measured
against MATLAB R2025b. The 22 new cases, including numeric concatenation and
additional shapes/types, still require the orchestrator's MATLAB reference run.
Do not describe those as independently MATLAB-verified by this lane.

## Apply, inspect, revert

From this project:

```sh
python3 scripts/patch_packages.py --check
python3 scripts/patch_packages.py
python3 scripts/patch_packages.py --revert
```

Use `--project /absolute/project/path` to select another project explicitly.
The default package is `datatypes-1.5.0`; no packages are loaded or executed by
the patch script. The kernel does not apply patches automatically. The
orchestrator decides when to apply them to the user's actual package copy.
Already loaded Octave class definitions need a fresh session/explicit reset to
use modified files; this script never resets a session.

The script uses Python's standard library and an exact unified-diff reader,
with no external `patch` command. It reads and validates every target and saved
original before writing anything. A pristine hash applies the complete ordered
patch set; a patched hash skips; any unknown hash refuses the entire package.
`--check` reports `pristine` or `patched` without writing. Unknown content gives
a nonzero exit and a message identifying the target and SHA-256.

Original bytes are saved inside the package at
`.indymat-patches/pristine/string.m`, next to `.indymat-patches/registry.json`.
Each replacement is atomic, preserves existing file permissions, and an ordinary
write failure rolls back the transaction including metadata. Revert verifies
the saved original's hash, restores the target byte for byte, and retains the
registry/backup for auditing. An already pristine revert skips. Do not edit a
package concurrently with a patch transaction. Multiple file replacements cannot
be made crash-atomic with the standard filesystem API; this patch set has one
target, whose replacement is atomic. A crash may leave harmless metadata/temp
files, but cannot leave a partially written `string.m`.

Only files under `<project>/.packages/datatypes-1.5.0/` can be written. Absolute
paths, traversal, symlinked package roots, symlinked targets, and symlinked
registry/backup directories are rejected. The package registry
`.packages/octave_packages` and `scripts/relocate_packages.py` are untouched.

An upgrade has a different directory/version and is refused until a separately
reviewed patch set exists. A reinstall or local edit that changes any target
bytes is likewise refused, rather than trying a fuzzy patch. A byte-identical
reinstall of the pinned original can safely be patched again. An already patched file
without its saved original is skipped by apply/check, but revert is refused
because the original cannot be safely restored. Never adjust the
hashes just to make a different release apply.

## Supported subset and limits

Mixed calls use the project's existing compat helpers. The bridge converts
later scalar strings to char and arrays to cellstr, rejecting missing later
strings with `<function>: missing string arguments are not supported`. No
string object remains when it redispatches, so it cannot loop back into the
class. `split` and `join` subsequently invoke the existing string-first method
through the helper, where the guard does not run.

Existing helper restrictions continue to raise their English errors:

- `contains`/mixed `count`: literal patterns; optional logical scalar
  `IgnoreCase`; non-ASCII IgnoreCase is unsupported. Mixed `count` rejects empty
  patterns. String-first count's existing empty-pattern result remains unchanged.
- `replace`/`erase`: nonempty literal patterns; scalar or same-shaped replacement
  text. No new regex or pattern-object support.
- Mixed `compose`: real 2-D numeric/logical data, char rows, and later scalar
  strings. Non-scalar string data become cellstr and receive the helper's
  `compose: data must be real 2-D numeric/logical arrays or char row vectors`
  error. Dynamic widths, positional/subtype formats, numeric-to-text coercion,
  and qualified non-ASCII text formats remain unsupported. Nonempty data require
  a char format, as in the existing helper.
- `split`: nonempty literal delimiters, optional positive integer dimension,
  optional delimiter output. `join`: literal delimiters, optional positive
  integer dimension; explicit empty scalar delimiter is supported, empty
  delimiter arrays are unsupported.
- `plus`: string operands, char rows/empty char, or the bounded numeric scalar
  subset above. All other non-string operands raise
  `string.plus: non-string operands must be char vectors or finite integer numeric scalars between -9999 and 9999.`

String-first methods keep their existing code paths except for empty-pattern
contains, non-overlap count, and the newly accepted plus operands. In particular,
count still sums per-pattern counts; interactions between overlapping *different*
patterns have not been measured here. Existing package limitations are not
claims of full MATLAB compatibility.

No helper change is required for the existing six mixed-argument failures.
To support mixed compose with non-scalar string data, the exact future helper
change would be an `elseif (iscellstr (a))` normalization branch assigning
`data{k} = a` in `octave/compat/compose.m`, plus row/column/empty shape tests and
MATLAB measurement. That helper is outside this lane's ownership and was not
edited.

See [REPORT.md](REPORT.md) for reproduction and verification results.
