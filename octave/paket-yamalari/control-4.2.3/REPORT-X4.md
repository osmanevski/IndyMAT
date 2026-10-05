# Wave 15 — X4 engineering differences

No text functions, probe inputs, recorded MATLAB results, baselines, scripts, application code, or Git state were changed. All numerical work used short-lived Octave processes and this worktree’s private package copy. The source hashes identify the supplied installed files; they are not a claim about every upstream installation with the same version.

## Results

| Scope | Probes | Before differences | After differences | Previously matching regressions |
|---|---:|---:|---:|---:|
| Engineering ys- groups owned by X4 | 204 | 48 | 25 | 0 |
| Whole ys- prefix (includes X3 text groups) | 455 | 100 | 77 | 0 |
| All differential probes | 1310 | 192 | 169 | 0 |

23 measured differences fixed; 5 require representation/class work (`çekirdek`); 20 left with reasons below. Both engines throwing errors is counted as matching by scripts/fark.py; error-message equivalence is not claimed.

## Commands and verification

```sh
python3 scripts/patch_packages.py --package control-4.2.3
python3 scripts/patch_packages.py --package signal-1.4.8
python3 scripts/patch_packages.py --package statistics-2.0.0
python3 -m unittest tests.test_engineering_helpers -v
python3 scripts/fark.py --octave-only --only ys-
python3 scripts/fark.py --octave-only
python3 scripts/compat.py --check
```

All six added tests pass. They check exact reconstruction of every pinned patch target, warm pass-through equivalence/overhead, percentile methods/dimensions/types/empty shapes, rational bandwidth/state order, empty windows and envelope offset invariance, and numeric identities for package changes. FIR integer orders retain the floating half-sample center.

`compat.py --check`: 572/703 cases pass; the only baseline regressions are the 62 graphics/GUI cases listed below, whose figure creation cannot run in this sandbox. The two pwelch cases now pass. No numerical baseline regression occurred. The command exits 1 because of those graphics limitations, so a fully green integration gate is still required outside the sandbox.

## Shadow overhead

Warm median of three trials, 200 calls per trial on small inputs; baseline is a bound original m-file handle. Units are microseconds per call. Resolver caches centrally because temporary path precedence can invalidate another shadow’s local persistent cache. First binding can take about 0.45 seconds on this installation and restores the full original path. Octave 11.3 does not implement `which("-all", name)`; `file_in_loadpath(name + ".m", "all")` supplies the complete path listing, skipping compat.

| Shadow | Original call | Added warm overhead | Unchanged path tested |
|---|---:|---:|---|
| bandwidth | 20.134 | 19.691 | Numeric matrix, lower/upper outputs |
| hamming | 12.840 | 19.740 | Nonzero window length |
| blackman | 16.454 | 19.885 | Nonzero window length |
| prctile | 164.970 | 21.191 | Floating inputs without Method |
| quantile | 136.786 | 16.755 | Calls without Method |

These are the only function shadows. `order` is new; `envelope` modifies an existing project helper. Package changes are in-place versioned source patches, with no extra shadow layer. Core numeric operators and class dispatch were not intercepted.

## Every initial difference in this slice

| id | kind before | outcome | file or reason |
|---|---|---|---|
| `ys-tf-properties` | Octave hata verir | düzeltildi | control: 01-sys_keys, 02-get, 03-set; Numerator/Denominator aliases preserve padded coefficients and setter validation. |
| `ys-zpk-construction` | değer farklı | çekirdek | Octave zpk constructs a tf object. A dedicated zpk representation and class dispatch are outside this slice. |
| `ys-zpkdata-outputs` | değer farklı | çekirdek | The tf representation discards the constructor root ordering; sorting computed roots cannot generally restore MATLAB zpk data. |
| `ys-bandwidth` | Octave hata verir | düzeltildi | octave/compat/bandwidth.m; first -3 dB crossing for continuous real rational SISO models, found from the magnitude-squared polynomial. |
| `ys-minreal` | boyut farklı | düzeltildi | control: 06-minreal; use MATLAB default sqrt(eps) cancellation tolerance instead of the relaxed 1000-fold relative threshold. |
| `ys-lsim-output` | değer farklı | bırakıldı | MATLAB automatically chose ZOH for this nonsmooth signal; Octave defaults to FOH. The general automatic selection criterion is unverified; changing all defaults would break smooth inputs. |
| `ys-bode-output` | değer farklı | düzeltildi | control: 04-bode; SISO outputs 1×1×N, frequencies N×1; plotting is unchanged. |
| `ys-nyquist-output` | değer farklı | düzeltildi | control: 05-nyquist; SISO outputs 1×1×N, frequencies N×1; plotting is unchanged. |
| `ys-stepinfo-fields` | değer farklı | bırakıldı | Automatic time-grid/end-time selection differs. Do not tune a grid to this first-order probe. |
| `ys-pid-class` | Octave hata verir | çekirdek | Octave pid is a tf and pidtune is absent; a real PID class plus tuning algorithm cannot be supplied by a small package patch. |
| `ys-isstable-order` | Octave hata verir | düzeltildi | octave/compat/order.m; SS state count or SISO TF denominator degree without cancellation. |
| `ys-margin-outputs` | değer farklı | bırakıldı | DC gain crossing and phase-wrap convention differ. The general DC/tangency boundary rule is not established. |
| `ys-tf-zpk-ss-convert` | değer farklı | çekirdek | zpk(sys) retains Octave tf class; requires an actual zpk class rather than a class-name shim. |
| `ys-ellip-coeff` | değer farklı | bırakıldı | ncauer uses a truncated elliptic nome expansion and approximate stop-band solve. Tightening the solve did not fix poles; no partial precision patch is retained. |
| `ys-besself-coeff` | boyut farklı | düzeltildi | signal: 02-besself; pad numerator to denominator length in the two-coefficient-output form. |
| `ys-fir1-default` | değer farklı | düzeltildi | signal: 01-fir1; exact windowed-sinc band integrals instead of sampled fir2 design; existing scale rule. |
| `ys-fir1-window` | değer farklı | düzeltildi | signal: 01-fir1; same algorithm with the supplied window. |
| `ys-fir2-basic` | değer farklı | bırakıldı | Frequency-grid gridding/interpolation convention differs; the general MATLAB grid rule was not established. |
| `ys-firpm-basic` | değer farklı | bırakıldı | Equiripple exchange/convergence differs slightly; no coefficient fitting or relaxed differential tolerance. |
| `ys-pwelch-size` | Octave hata verir | düzeltildi | signal: 05-pwelch; default to the existing R12+ sample-overlap/non-detrended MATLAB convention; explicit compatibility switches remain. |
| `ys-resample-values` | değer farklı | düzeltildi | signal: 04-resample; default order 20*max(p,q), Kaiser beta 5, normalized DC gain p; supplied-filter polyphase path remains. |
| `ys-interp-values` | yalnız MATLAB hata verir | bırakıldı | Only MATLAB errors for this short input; left per the rule. Its already-different Octave result shifts as it uses corrected fir1. |
| `ys-findpeaks-height` | Octave hata verir | bırakıldı | Package third output is a parabola-fit structure; MATLAB returns width and prominence as numeric third/fourth outputs. Needs a full prominence/width implementation. |
| `ys-envelope-default` | Octave hata verir | bırakıldı | Numeric FIR analytic-filter length is unsupported; Hilbert FFT envelope is not an equivalent substitute. |
| `ys-tf2ss` | değer farklı | düzeltildi | signal: 03-tf2ss; proper numeric polynomial inputs with up to four outputs use controllable companion form, retaining cancelled states. Other forms delegate to dssdata. |
| `ys-sos2tf` | boyut farklı | bırakıldı | Measured MATLAB removes one numerator trailing zero here. The general zero/delay preservation rule is unverified; trimming all zeros would change filter values. |
| `ys-residuez` | değer farklı | bırakıldı | Root/residue ordering differs; arbitrary reversal or sorting is not verified for complex and repeated poles. |
| `ys-buttord` | değer farklı | bırakıldı | MATLAB chooses a different valid cutoff within the design interval; general pass/stop constraint policy is unverified for high-pass and multiband cases. |
| `ys-cheb2ord` | değer farklı | bırakıldı | Returned stop-band cutoff differs. General edge-selection policy is not established; no scalar-probe-only assignment. |
| `ys-window-zero` | Octave hata verir | düzeltildi | octave/compat/hamming.m, blackman.m; signal: 06-hann; empty window is double 0×1. |
| `ys-pwelch-default-size` | yalnız MATLAB hata verir | bırakıldı | Only MATLAB errors for eight-sample default input; left per the rule, even though pwelch conventions were corrected. |
| `ys-envelope-rms` | değer farklı | düzeltildi | octave/compat/envelope.m; center input before moving RMS and restore mean in both envelopes. |
| `ys-grpstats-output` | Octave hata verir | düzeltildi | statistics: 02-grpstats; numeric sum aggregation and fewer requested outputs. |
| `ys-poisspdf-zero` | değer farklı | düzeltildi | statistics: 01-poisspdf; zero rate is the degenerate distribution at zero, retaining type and scalar expansion. |
| `ys-prctile-integer` | sınıf farklı | düzeltildi | octave/compat/prctile.m; interpolate in double and cast result back to input integer type. |
| `ys-zpk-static` | değer farklı | çekirdek | Static zpk is represented as tf; requires the same dedicated class work. |
| `ys-stepinfo-extra` | değer farklı | bırakıldı | Same automatic time-grid/end-time difference, including finite-horizon Peak and PeakTime. |
| `ys-ellip-zpk` | değer farklı | bırakıldı | Same ncauer issue. A complete, independently checked elliptic prototype algorithm is needed. |
| `ys-fir1-highpass` | değer farklı | düzeltildi | signal: 01-fir1; exact high-pass band integral, existing Nyquist normalization. |
| `ys-fir1-bandpass` | değer farklı | düzeltildi | signal: 01-fir1; exact band integral, existing passband-center normalization. |
| `ys-pwelch-frequency-vector` | Octave hata verir | bırakıldı | Package has FFT-size input only; arbitrary-frequency Welch evaluation remains unimplemented. |
| `ys-periodogram-frequency-vector` | Octave hata verir | bırakıldı | Octave core periodogram lacks arbitrary-frequency input. No FFT interpolation substitute is used. |
| `ys-findpeaks-distance` | değer farklı | düzeltildi | signal: 07-findpeaks; reject peaks at the MinPeakDistance boundary as well as inside it. |
| `ys-findpeaks-prominence` | Octave hata verir | bırakıldı | MinPeakProminence requires that full prominence implementation; no approximate threshold substitute. |
| `ys-norminv-logtail` | Octave hata verir | bırakıldı | Recorded MATLAB accepts a fourth upper argument but returns the ordinary lower inverse. This conflicts with an assumed inverse-survival interpretation; leave until optional-argument semantics are confirmed. |
| `ys-prctile-method` | Octave hata verir | düzeltildi | octave/compat/prctile.m + __mf_quantile_options__.m; midpoint/inclusive/exclusive map to HF types 5/7/6. |
| `ys-quantile-method` | Octave hata verir | düzeltildi | octave/compat/quantile.m + __mf_quantile_options__.m; same named-method mapping; unnamed forms unchanged. |
| `ys-grpstats-size` | Octave hata verir | düzeltildi | statistics: 02-grpstats; requesting the first statistic of a longer list is permitted. |

## Supported subsets and remaining uncertainties

`bandwidth` supports continuous real rational SISO transfer functions/state-space models and finite negative dB drops. Discrete, FRD, complex and MIMO models raise explicit errors. Polynomial root conditioning remains a numerical limitation for high-order models. `order` supports SS and SISO TF only. Named percentile methods support midpoint/inclusive/exclusive (HF 5/7/6); no approximated/weighted method is advertised. The RMS differential evidence is the odd three-sample window probe; even windows use the standard centered convention with one extra preceding sample. Other envelope modes are unchanged.

Bode/nyquist patches retain the package’s SISO-only limit and change only returned shapes; they do not add MIMO frequency-response handling. Transfer-function property aliases retain Octave setters, including the existing static-gain setter restriction. The package’s `pwelch` default now uses its built-in MATLAB R12+ mode (sample overlap, no default detrending); arbitrary frequency vectors remain unsupported. Explicit Octave mode switches are preserved. The default resampling filter is changed; explicit supplied filter coefficients retain the original computation. `tf2ss` retains the original package path for other call forms and fifth-output descriptor requests.

No retained patch depends on fitted probe coefficients. No claim of a complete toolbox implementation or independently rerun MATLAB reference is made. The larger algorithms and ambiguous ordering/option cases listed above remain visible.

## Sandbox graphics/GUI baseline exceptions

`arayuz-guidata:`, `arayuz-uicontrol:`, `grafik-cift-y-ekseni:`, `grafik-cizgi-nesnesi:`, `grafik-coklu-cizgi-hold:`, `grafik-eksen-sinirlari:`, `grafik-etiketler:`, `grafik-exportgraphics:`, `grafik-fplot-cizim:`, `grafik-heatmap:`, `grafik-histogram-cizim:`, `grafik-logaritmik:`, `grafik-patch-ve-doldurma:`, `grafik-polarplot:`, `grafik-renk-haritasi:`, `grafik-renk-sirasi:`, `grafik-sabit-cizgiler:`, `grafik-subplot:`, `grafik-temel-turler:`, `grafik-tiledlayout:`, `grafik-uc-boyut:`, `grafik-ust-baslik:`, `grafiky-colororder-current:`, `grafiky-colororder-explicit-color:`, `grafiky-colororder-implicit-figure:`, `grafiky-colororder-recolor-cycle:`, `grafiky-exportgraphics-axes:`, `grafiky-exportgraphics-callback:`, `grafiky-exportgraphics-decorations:`, `grafiky-exportgraphics-figure:`, `grafiky-exportgraphics-pdf:`, `grafiky-exportgraphics-preserves-source:`, `grafiky-exportgraphics-tiff:`, `grafiky-nexttile-current:`, `grafiky-nexttile-deleted-slot:`, `grafiky-nexttile-first-free:`, `grafiky-nexttile-free-span:`, `grafiky-nexttile-obsolete-layout:`, `grafiky-nexttile-reselect:`, `grafiky-nexttile-reuse:`, `grafiky-parula-figure-size:`, `grafiky-parula-varsayilan:`, `grafiky-polarplot-cizgi:`, `grafiky-polarplot-color:`, `grafiky-polarplot-property-first:`, `grafiky-polarplot-temel:`, `grafiky-renk-sirasi-adlar:`, `grafiky-renk-sirasi-axes:`, `grafiky-renk-sirasi-figure:`, `grafiky-renk-sirasi-figure-axes:`, `grafiky-tiledlayout-grid:`, `grafiky-tiledlayout-index:`, `grafiky-tiledlayout-replaces-axes:`, `grafiky-tiledlayout-spacing:`, `grafiky-tiledlayout-span:`, `grafiky-yyaxis-cla-reset:`, `grafiky-yyaxis-left-right:`, `grafiky-yyaxis-limitler:`, `grafiky-yyaxis-ruler-after-plot:`, `grafiky-yyaxis-shared-hold:`, `kontrol-cizimler:`, `soz-command-hold:`
