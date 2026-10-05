# signal 1.4.8 — engineering compatibility patches

Versioned GPL package source modifications; the manifest pins both original and
patched SHA-256 hashes. No compiled files or dependencies are changed.

| Patch | Effect |
|---|---|
| 01-fir1 | Exact windowed-sinc ideal band integrals replace the sampled fir2 design. Existing windows, odd-order adjustment and normalization are retained. Integer orders retain half-sample centers. |
| 02-besself | Two coefficient outputs have equally long numerator and denominator rows, adding leading numerator zeros. |
| 03-tf2ss | Proper numeric polynomial inputs and up to four outputs use controllable companion form, retaining cancelled modes. Other forms and descriptor/fifth-output requests use the original dssdata path. |
| 04-resample | Default filter has order 20*max(p,q), Kaiser beta 5, unit DC normalization followed by interpolation gain p. Explicit supplied filter coefficients retain the original polyphase path. Unity resampling is exact. |
| 05-pwelch | Select the existing R12+ MATLAB convention by default: overlap in samples, no default detrending. Explicit native/R11/R12+/psd mode switches remain. |
| 06-hann | Zero-length windows return double 0×1; other lengths use existing hanning. |
| 07-findpeaks | MinPeakDistance excludes candidates at the boundary too. Existing forms retain parabolic width estimation and the package third-output structure. |
| 08-ellip-wave16 | For positive ripple specifications with a degree equation bracketed in double precision, use the Jacobi elliptic prototype in `__mf_ellip_prototype__`. This corrects poles and gain as well as zeros; the existing frequency transformations and output forms remain. Degenerate/unbracketed specifications use the old prototype. |
| 09-fir2-wave16 | Default, strictly increasing band edges use MATLAB's integer grid-bin interpolation and repeated Nyquist endpoint. Explicit grids/windows, discontinuous edges, and coincident bins retain the old interpolation. |
| 10-buttord-wave16 | Scalar pass/stop edges select the stopband-matching cutoff for one/two outputs. Analog and digital, lowpass and highpass are covered. Band forms and the three-output extension remain. |
| 11-cheb2ord-wave16 | The same scalar-edge subset returns the stopband-matching cutoff. Band forms and the three-output extension remain. |
| 12-residuez-wave16 | Real simple poles and corresponding residues are ordered by decreasing magnitude. Complex and repeated pole conventions remain. |
| 13-findpeaks-wave16 | Four-output calls and `MinPeakProminence` filtering use finite real double-vector prominence and half-prominence widths. Supports height/distance/width/threshold filters, sorting, and NPeaks. Explicit locations, sample rates, Inf, DoubleSided and other width references are unsupported. Other two/three-output forms retain wave 15. |
| 14-pwelch-wave16 | In R12+ mode, short records reject default windows with MATLAB's identifier. Explicit frequency vectors with an explicit window use direct two-sided Welch estimates via `__mf_pwelch_frequency__`; sample overlap and optional sample rate are supported. Default windows in that new form and extra options remain unsupported. Scalar FFT lengths and mode switches remain. |

```sh
python3 scripts/patch_packages.py --package signal-1.4.8 --check
python3 scripts/patch_packages.py --package signal-1.4.8
python3 scripts/patch_packages.py --package signal-1.4.8 --revert
```

The script validates the complete set before writing, installs each replacement
atomically, preserves original bytes and refuses unknown hashes/versions or
symlinks. Apply to an idle private copy and start a fresh session; the patcher
never resets the user's session. Pinned source identities are those supplied
with this project, not a claim about all upstream installations of this version.

See the [X4 report](../control-4.2.3/REPORT-X4.md) for wave 15 evidence.
Wave 15's attempted elliptic precision patch was not retained because improving
the stop-band solve did not establish correct poles and gain. Wave 16 replaces
the approximate prototype within the supported subset; see the
[Y4 report](REPORT-Y4.md) for verification, overhead and remaining differences.
