# indy-graphics oracle: measured Octave behaviour

Source: GNU Octave 11.3.0 (`octave-cli`, fltk toolkit, invisible figures, default
figure 560x420 px). Black-box only: public plotting calls plus `get()` on the
resulting objects. No Octave source was read. Every number below comes from
`measure.json` or `<case>.json` in this directory. Formulas marked **fit** were
inferred from the data and checked against every measured point I had. Items
marked **hypothesis** are not established.

## Files

- `../../../../scripts/grafik_oracle.sh [out_dir] [case ...]` runs each `%% name`
  case of `cases.txt` in its own fresh Octave process (fltk, `figure('visible','off')`,
  body evaluated with `evalc`) and writes `jsonencode(__mf_figure_data__(gcf))` plus a
  newline to `<name>.json`. If the body throws, it writes `<name>.error.txt` instead.
  Stale outputs for a case are deleted before it runs. `GRAFIK_TOOLKIT=qt` runs the
  same cases under Qt (`octave --no-gui`, `QT_QPA_PLATFORM=offscreen`); use that only
  for cross-checks. Helper: `grafik_oracle.m`.
- `../../../../scripts/grafik_measure.sh [out_dir]` writes `measure.json` from
  `grafik_measure.m`. It takes about 3 minutes, most of it in the two dense tick sweeps.
  Top-level keys: meta, ticks, ticks_sweep, ticks_fine, ticks_size, tick_labels,
  axis_modes, subplot, colorbar, camera, colors, defaults, geometry, clim. Vectors are
  always JSON arrays. Matrices are lists of rows.
- Determinism: two full oracle runs gave byte-identical output.

All 50 cases pass. `empty_figure` gives `supported:false, reason_code:"no_axes"`. That
is the extractor's normal answer for a figure with no axes, not an error.

## 1. Auto limits and ticks (linear axes): fit, 12512/12514 points exact

Data: 322 hand-picked or random points (`ticks`, both x and y), 4584 points from
`ticks_sweep`, 7608 from `ticks_fine`, and 88 from `ticks_size`. Values are compared
bit for bit as doubles.

```
nice(r):                      # step for a range r > 0; target = 5 intervals
  q = r/5; e = floor(log10(q)); f = q / 10^e
  m = 1 if f < sqrt(2) else 2 if f < sqrt(10) else 5 if f < sqrt(50) else 10
  return m * 10^e             # 10^e = correctly rounded double (== parse("1e<e>"))
                              # i.e. the {1,2,5}*10^k nearest to r/5 in log space
auto(lo, hi):                 # lo/hi = data min/max over the axis
  if lo == hi: (lo,hi) = (-1,1) if lo == 0 else (lo-0.1|lo|, hi+0.1|hi|)
  repeat 2 times:             # exactly twice, recomputing the step each time
     s = nice(hi-lo); lo = floor(lo/s)*s; hi = ceil(hi/s)*s
  ticks = (k0+i)*s, k0 = round(lo/s), i = 0..round((hi-lo)/s)
```

- The second pass matters because of floating-point creep. For example
  `-0.5/0.2 = -2.5` gives `-0.6000000000000001`, and the second pass turns that into
  -0.8. So `plot([-0.5 0.5])` gets ylim `[-0.8 0.8]` with 9 ticks. Also
  `[0,0.51] -> [0,0.7]`, `[0,1.01] -> [0,1.4]`, `[0,1.45] -> [0,2]` (step 0.5).
  A third pass would be wrong: tested, 83 mismatches.
- Tick values must be `(k0+i)*s`. `lo+i*s` gave 62 mismatches and accumulation gave 80.
- The tick count does not depend on axes pixel size. Default, subplot(2,2,1),
  subplot(4,1,1), subplot(1,4,1) and figures of 300x200, 1200x900, 200x800 and
  1600x300 all gave identical lim/tick for 11 ranges (`ticks_size`). Resizing after
  plotting changes nothing.
- Unexplained residual: the single point `plot(0.3,0.3,'o')` gives `[0.26 0.33]`.
  The formula gives `[0.27 0.33]`. All other single points match: 5, 0, -3, 1e-5, 1000.
- Manual or tight limits (xlim/ylim/axis/axis tight, colorbar): the limits are kept as
  given and ticks are `k*s` for `k = ceil(lo/s)..floor(hi/s)` with `s = nice(hi-lo)`.
  This was verified on line_limits, axis_manual, axis_tight, axis_equal, every colorbar
  in `measure.json`, and image axes.
- `axis tight` sets the limits to data min/max exactly, and xlimmode still reads
  'auto'. Image axes use `[0.5, n+0.5]` with `ydir reverse`. `bar` sets
  `xtickmode manual` with ticks at the bar x positions.

## 2. Tick labels: fit, 2194/2195 labels

- `label = sprintf('%.5g', v)` if `|v| >= 1`, else `sprintf('%.4g', v)`. Examples:
  0.12345 gives '0.1235', 1/3 gives '0.3333', 12345.6 gives '12346', 123456 gives
  '1.2346e+05', 1e5 gives '1e+05', 2e5 gives '2e+05', 99990 gives '99990', 1e-5 gives
  '1e-05', 0.0001 gives '0.0001', 0.99999 gives '1'.
- Exception: 100005 gives '1.0000e+05', where C `%g` would give '1e+05'. It looks like a
  half-way rounding case that keeps its zeros. Not understood.
- There is no exponent multiplier ("x10^n") anywhere. Labels carry the exponent
  themselves, so neighbouring ticks can collide: ylim [1e6 1.0001e6] gives six labels
  all '1e+06'. No visible text object holds an exponent: `findall` finds only the 4
  title/label texts. No axes property name contains exp/ruler/offset.

## 3. Axis modes (`axis_modes`, line `plot([0 3],[0 1])`, default figure)

- `axis equal` (2D): sets `dataaspectratio=[1 1 1]` manual and
  `plotboxaspectratio=[w_px h_px h_px]` manual, where `[434 342.3]` is the axes pixel
  size (0.775*560, 0.815*420). The limits on the over-sized axis are stretched about
  their centre: `ylim = 0.5 ± (3*342.3/434)/2 = [-0.68306, 1.68306]`. Position does not
  change. Imagesc behaves the same way.
- `axis equal` (3D, surf/plot3): DAR [1 1 1] manual, PBAR [1 1 1] manual. All three
  ranges become the largest range, centred: `[-0.5 4.5],[-1 4],[1 6]`.
- `axis image`: DAR [1 1 1] manual, limits unchanged, PBAR is auto with value
  `[xr, yr, *]` (for example `[3 1 2.366]`). Position does not change, so a renderer
  must letterbox the box (**hypothesis**).
- `axis square`: PBAR [1 1 1] manual. `vis3d`: DAR and PBAR manual. `normal` and
  `auto` revert. `manual`: limmode manual. `axis off`: axes `visible='off'`, nothing
  else changes. `fill`: no visible change.

## 4. Subplot positions: fit, exact for every grid measured

With the figure W x H px (default 560x420) and the default inner box
`[0.13 0.11 0.775 0.815]`:

```
gx = 0.2/n + 21.25/W ; w = (0.775 - (n-1)*gx)/n ; x(j) = 0.13 + (j-1)*(w+gx)
gy = 0.2/m + 21.25/H ; h = (0.815 - (m-1)*gy)/m ; y(i) = 0.11 + (m-i)*(h+gy)  # i=1 top row
```

- Checked on 1x1, 1x2, 2x1, 2x2, 3x1, 1x3, 3x3, 2x3, 4x1, 1x4, 4x4 and on a 300x900
  figure. A subplot with a span, such as (2,2,[1 2]) or (3,3,[1 5]), takes the bounding
  box of its cells.
- Positions are the same with or without data, and title/xlabel/ylabel do not change
  them. The extractor's position equals `get(ax,'position')`. The 21.25 px constant may
  depend on font size (**hypothesis**, not varied).
- Outerposition is recorded but not modelled. Note that `subplot(1,1,1)` has
  OP=[0.0854 0.0612 0.8196 0.8876] while `axes()` has [0 0 1 1].
- `tiledlayout` and `nexttile` do not exist in this Octave (`exist`=0), so nothing was
  measured for them.

## 5. Colorbar: fit, exact

Let the peer position be `[x y w h]`:

| location | peer after | colorbar |
|---|---|---|
| eastoutside (default) | [x y 0.8w h] | [x+0.9w, y, 0.06w, h] |
| westoutside | [x+0.2w y 0.8w h] | [x, y, 0.06w, h] |
| northoutside | [x y w 0.8h] | [x, y+0.9h, w, 0.06h] |
| southoutside | [x y+0.2h w 0.8h] | [x, y, w, 0.06h] |
| east / west | unchanged | [x+0.9w / x+0.05w, y+0.05h, 0.054w, 0.9h] |
| north / south | unchanged | [x+0.05w, y+0.9h / y+0.05h, 0.9w, 0.054h] |

- The layout does not depend on figure size (300x200, 1200x900) and also holds for
  subplot peers. Deleting the colorbar restores the peer to its original position.
- Colorbar limits equal `clim` exactly. Ticks follow the manual-limits rule (§1):
  [1 6] gives 1..6, [0 1] gives 0:0.2:1, [-3 7] gives -2:2:6, [1 9] gives 2:2:8,
  [0 255] gives 0:50:250. Calling `caxis` before or after `colorbar` makes no difference.
- Vertical bars have `xlim [-0.5 1.5]` and `xtick []`. Horizontal bars have
  `ylim [-0.5 1.5]` and `ytick []`.
- Measured axis locations: eastoutside y-axis right, westoutside left, east (inside)
  left, west right; northoutside x-axis top, southoutside bottom, north bottom,
  south top.

## 6. Camera: fit, exact on 64 states (45 come from a view sweep)

Let `c` = centre of the limits, `R` = limit ranges, `P` = plotboxaspectratio. Then:

- `dataaspectratio = (R./P) / min(R./P)` (auto mode).
- `cameratarget = c`.
- `d = [sin(az)cos(el), -cos(az)cos(el), sin(el)]`.
- `cameraposition = c + 5*norm(P) * d .* (R./P)`, with the x term negated when
  `xdir=reverse` (this was measured for x only).
- `cameraviewangle = 2*atand(E / (5*norm(P)))`, where `E` is the larger of the half
  width and half height of the box `P` projected onto the view plane. 2D gives
  6.608610360311924, the default 3D view gives 10.339584907201973, view(45,60) gives
  11.3733, view(120,-20) gives 9.28759. The angle does not depend on figure or subplot
  pixel shape. `projection perspective` leaves position and angle as they are.
- `cameraupvector = [0 0 1]` (not scaled) when `|el| < 90`. At el=90 it is
  `[-sin(az), cos(az), 0].*(R./P)`, so 2D `plot([1 2 4],[3 5 4])` gives `[0 2 0]`.
  At el=-90 it is `[sin(az), -cos(az), 0].*(R./P)`.
- 2D zlim is [-1 1].

## 7. Colours and colormaps (`colors`, full precision in JSON)

- The default colororder has 7 rows: [0 .447 .741; .85 .325 .098; .929 .694 .125;
  .494 .184 .556; .466 .674 .188; .301 .745 .933; .635 .078 .184]. The cycle wraps
  after 7. Under `hold on`, an explicit colour such as 'r' still advances the cycle:
  colours go 1, 2, red, 3.
- The default figure colormap is 256x3 and equals `viridis(256)` exactly.
- `jet()`, `gray()` and `colormap(gray)` all give 256 rows. jet(8), gray(4), hot(4),
  viridis(4) and turbo(4) are in `colors`. `parula` does not exist.
- `clim` auto (`clim.cases`): it is [min max] over scaled colour data from surface,
  imagesc, indexed patch and scalar-cdata scatter. Lines and direct images add nothing,
  and with no colour data it is [0 1]. Constant data c gives [c-1, c+1].
- A patch with no colour data (bar, area, or fill with RGB) counts as the value 0. That
  is why every bar, area or RGB-fill figure shows `clim [-1 1]`, including
  `tests/fixtures/figure_v3/bar_grouped.json`. bar+surf(5..8) and fill+imagesc(5..8)
  both give [0 8]. Contour gives [first drawn level, last level] and contourf gives
  [min level, last level].

## 8. Defaults (`defaults`)

- line: linewidth 0.5, markersize 6, markerfacecolor 'none', markeredgecolor 'auto'.
  This holds for `plot(..,'o')` and for `line()`, and both pick the colour from the
  colororder.
- scatter: sizedata 36, marker 'o', face 'none', edge 'flat', cdata = next colour.
  `'filled'` gives face 'flat' and edge 'none'.
- bar: hggroup, barwidth 0.8, face = colororder per series, edge black, basevalue 0.
  The baseline is a hidden black line spanning xlim, and it is a child of the first
  series.
- area: face = colororder, edge black. stem: marker 'o', face 'none', and a black
  baseline line under the axes spanning xlim.
- text: fontsize 10, margin 3, left/middle, tex, units data, black, no box. Axes:
  fontsize 10 (title and label 11, using multiplier 1.1, title bold), box off for
  plot/surf and on for imagesc, ticklength [.01 .025], tickdir in, grid colour
  0.15/alpha 0.15. surf turns on x/y/z grid.
- legend: location 'northeast', vertical, box on, fontsize 9. The axes do not shrink.
- figure: white, [300 200 560 420] px. Axes: [0.13 0.11 0.775 0.815].

## 9. Geometry (`geometry`; x spacing = minimum distance between bar x positions)

- bar, single series: patch per series, 4 vertices per bar:
  `[xl b; xl top; xr b... ]` in the order (xl,b),(xl,t),(xr,t),(xr,b), with half-width
  `bw/2*spacing` and `b = basevalue`.
- bar, grouped with n ≥ 2 series (fit from n=2..5 and bw=0.4..1, plus the fixture
  bw=0.6): pitch `p = bw(1+bw)/(2n)`, width `w = bw(3bw-1)/(2n)`, centre
  `x + (k-(n+1)/2)p`, all scaled by the spacing. Example: n=2, bw=0.8 gives centres
  ±0.18 and width 0.28.
- bar, stacked: width bw. Positive and negative values stack separately from 0. barh
  swaps the axes.
- area: one patch per series with 2n+1 vertices:
  (x1,L1),(x1,U1)..(xn,Un),(xn,Ln)..(x1,L1). L is the previous cumulative series, or
  the basevalue for the first.
- stairs: an hggroup holding one line with x = x1,x2,x2,..,xn,xn and
  y = y1,y1,y2,y2,..,yn (2n-1 points).
- stem: an hggroup holding a marker line (linestyle none) and a stem line
  `[x x NaN]/[base y NaN]`, plus a separate baseline line.
- contour and contourf: `levelstep = (zmax-zmin)/11` and `levellist = zmin + (0:10)*step`.
  zmax itself is never a level. This was checked on 5 data sets.
- contour draws one patch per level above zmin. Each patch has facecolor 'none',
  edgecolor 'flat', a NaN terminator vertex, and facevertexcdata = the level.
- contourf draws 11 filled patches (including the zmin level) with black edges. Full
  vertices and faces are in `geometry.contour` and `geometry.contourf`.

## 10. fltk vs Qt

- `uret.m` was rerun with the current extractor under fltk and under Qt (offscreen) for
  all 39 recipes. There are 0 differing fields, `source.figure_size` included
  ([560 420] for both).
- All 50 `cases.txt` cases were also run under both toolkits: 0 differing fields.
- The committed Qt fixtures differ from either run only in fields that come from the
  extractor version, not the toolkit:
  - missing `limits.image_pixels`, `pixel_count` and `source.figure_size`;
  - on some files, missing `limits.text_lines/text_chars` and the per-axes
    `visible`, `interpreters`, `x/y/z tickmode` and `ticklabelmode`;
  - `json_budget.reason_args.actual` (8398934 vs 8399251 bytes);
  - `unsupported_object.reason_args.type` ('patch' vs 'image').
- No numeric value differs. So fltk can stand in for Qt for everything measured here.

## Not measured, or unknown

- Log-scale ticks, datetime and categorical axes, minor ticks.
- Text extents and font metrics. Legend size depends on them, so only the position
  value is recorded.
- What the 21.25 px subplot constant depends on.
- tiledlayout (it does not exist).
- `cameraupvector` with `zdir/ydir` reverse.
- The 0.3 single-point tick residual and the '1.0000e+05' label.
- Bar groups with uneven x spacing.
