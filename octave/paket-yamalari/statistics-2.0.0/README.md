# statistics 2.0.0 — engineering compatibility patches

Small changes to supplied GPL package source, pinned to exact original and
patched SHA-256 hashes in the manifest.

| Patch | Effect |
|---|---|
| 01-poisspdf | Lambda=0 is the point mass at X=0. Negative/NaN rates and NaN X retain invalid/missing results; type preservation and scalar expansion remain. |
| 02-grpstats | Numeric input accepts sum aggregation with NaN omission. Callers may request fewer leading outputs than the named statistics. Table aggregation is unchanged. |

```sh
python3 scripts/patch_packages.py --package statistics-2.0.0 --check
python3 scripts/patch_packages.py --package statistics-2.0.0
python3 scripts/patch_packages.py --package statistics-2.0.0 --revert
```

The patcher validates all hashes/diffs before writes, preserves original bytes,
uses atomic file replacements, and refuses unexpected files, symlinks or package
versions. Apply to an idle private package copy; use a fresh session afterward.
No user session is reset and no compiled files or dependencies are changed.
The pinned originals identify the supplied files, including prior project
changes, not every installation with this package version.

See the [X4 report](../control-4.2.3/REPORT-X4.md) for all differential results,
helper timing and remaining limitations.
