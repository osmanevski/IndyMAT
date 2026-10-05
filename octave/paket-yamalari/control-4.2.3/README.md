# control 4.2.3 — engineering compatibility patches

Small changes to the supplied GPL package source, pinned by original and final
SHA-256 hashes. `scripts/patch_packages.py` applies exact unified diffs atomically
and preserves verified original files for revert. No compiled files are changed.

| Patch | Effect |
|---|---|
| 01-sys_keys, 02-get, 03-set | Add Numerator/Denominator aliases to existing TF coefficient access and setters. Existing static-gain setter restrictions remain. |
| 04-bode | SISO magnitude/phase outputs are 1×1×N; frequencies are N×1. Plotting and the SISO-only limit remain. |
| 05-nyquist | Same output dimensions for real/imaginary response. |
| 06-minreal | Default pole-zero cancellation uses sqrt(eps), rather than the original 1000-fold relaxed relative tolerance. Explicit tolerance is unchanged. |
| 07-margin | Flatten bode's phase array at the package's existing interp1 call, preserving numerical margin computation with the new output dimensions. |
| 08-margin-dc-wave16 | Include zero-frequency gain crossovers and return their signed principal phase margin without interpolating at zero. Positive-frequency calculation remains. |

```sh
python3 scripts/patch_packages.py --package control-4.2.3 --check
python3 scripts/patch_packages.py --package control-4.2.3
python3 scripts/patch_packages.py --package control-4.2.3 --revert
```

The default patch command still selects datatypes; select each engineering
package explicitly. Apply only while the selected private package copy is idle.
A fresh session uses the modifications; the patcher does not reset a session.
Unknown file hashes, symlinks or versions are refused. The hashes identify the
supplied installed files, not every installation carrying this version number.

See [REPORT-X4.md](REPORT-X4.md) for differential counts, every initial difference,
helper subsets, timing, remaining limits and sandbox verification exceptions.
