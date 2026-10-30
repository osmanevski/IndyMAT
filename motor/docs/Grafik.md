# indy-graphics — MATLAB graphics without Octave

Status 29 Oct 2026, lane `r/grafik`. Crate `motor/crates/indy-graphics`:
a pure-Rust graphics object model whose output is exactly the Figure-Data
JSON (v2/v3, `docs/Figure-Data-v3.md`) that IndyMAT's viewer renders today.
No external crates, no Octave at runtime. Octave 11.3 (`octave-cli`, fltk
toolkit, headless) was used only as a black-box behaviour oracle; no GNU
Octave source was read. The serializer is a Rust port of IndyMAT's own
(MIT) extractor `octave/__mf_figure_data__.m` and helpers.

## Layout

| Module | Role |
|---|---|
| `model` | Object tree types: `Handle`, `Figure`, `Axes`, `Line`, `Scatter`, `Surface`, `Patch`, `Text`, `Image`, `Group` (bar/area/stem/stairs/contour), `Legend`, `Colorbar`; MATLAB defaults |
| `graphics` | `Graphics` session: storage, `figure gcf gca axes hold ishold cla clf close delete subplot tiledlayout nexttile`, `newplot` semantics |
| `plots` | `plot plot3 plot_log line scatter stairs stem bar area fill patch patch_faces surf mesh contour image text` |
| `annot` | `label` (xlabel/ylabel/zlabel/title) `legend grid box set_limits limits axis caxis colormap colorbar view` |
| `props` | `set`/`get` by MATLAB property name, every object type |
| `scene` | Flattening into drawable leaves (Octave drawing order), extents, automatic limits/ticks/CLim/aspect/camera |
| `layout` | The measured rules: tick step, two-pass limits, `%.5g` labels, log decades, subplot/colorbar geometry, camera |
| `emit` | Figure-Data serializer: version rule, budgets, rejection codes, all record kinds |
| `linespec` | `LineSpec` parser (`'r--o'`, `':sk'`, `'diamond'`, `;key;` suffix) |
| `color` | Default colour order, colour letters, colormaps `viridis jet gray hot cool copper bone spring summer autumn winter turbo hsv pink lines colorcube prism flag white cubehelix ocean rainbow` (`turbo.rs`: measured table) |
| `inset` | Subplot alignment around titles/labels (fitted text extents), drawn fixed-aspect plot box (lane r/grafik-3) |
| `contour` | Contour levels, line paths and filled rings (own implementation) |
| `callargs` | Interpreter helpers: MATLAB argument splitting, `take_changed` export tracking |
| `json` | Minimal JSON value, writer (`null` for nonfinite, shortest round-trip numbers), parser |

## API summary

All calls take plain Rust types; nothing depends on the interpreter's value
type. Errors are `GraphicsError { id, message }` with MATLAB-style ids.
Plotting calls take `ax: Option<Handle>` (`None` = current axes, created on
demand) and trailing `opts: &[(String, PropValue)]` name/value pairs applied
with `set` to every created object.

```rust
let mut g = Graphics::new();
let h = g.plot(None, &[PlotSeries::xy(&x, &y).with_spec("r--o")], &[opt("LineWidth", 2.0)])?;
g.label(None, LabelRole::Title, &["Kare".into()], &[])?;
g.set(h[0], "Color", PropValue::from([0.5, 0.5, 0.5]))?;
let json: String = g.figure_json(g.gcf());
```

`PropValue` = `Num | Array | Str | Cell | Handle | Handles | Bool`;
`Array` = dims + column-major `f64` data + `NumClass` (kept for CData).

## Coverage

Functions (brief list complete): `figure plot plot3 scatter scatter3 bar
barh stairs stem area fill patch surf mesh contour contourf imagesc image
xlabel ylabel zlabel title legend grid hold axis xlim ylim zlim subplot
colorbar colormap caxis text line close clf`, plus `gcf gca axes cla delete
ishold box view tiledlayout nexttile semilogx semilogy loglog set get`.

Properties (`set`/`get`, case-insensitive full names):

* common: `Type Parent Children Visible Tag DisplayName`
* figure: `Number Name Color Colormap Position NextPlot CurrentAxes`
* axes: `[XYZ]Lim[Mode] [XYZ]Tick[Mode] [XYZ]TickLabel[Mode] [XYZ]Scale
  [XYZ]Dir [XYZ]Grid [XYZ]MinorGrid [XYZ]Label Title View CLim[Mode]
  DataAspectRatio[Mode] PlotBoxAspectRatio[Mode] Camera{Position,Target,
  UpVector,ViewAngle}(get) Colormap ColorOrder ColorOrderIndex NextPlot
  Position Color Box FontSize TickLabelInterpreter Projection`
* line: `XData YData ZData Color LineStyle LineWidth Marker MarkerSize
  MarkerEdgeColor MarkerFaceColor`
* scatter: `XData YData ZData SizeData CData Marker MarkerEdgeColor
  MarkerFaceColor LineWidth`
* surface: `XData YData ZData CData FaceColor EdgeColor MeshStyle LineStyle
  LineWidth CDataMapping FaceAlpha EdgeAlpha Marker`
* patch: `Vertices Faces FaceVertexCData XData YData(get) FaceColor EdgeColor
  CDataMapping LineStyle LineWidth Marker FaceAlpha EdgeAlpha`
* text: `String Position Units FontSize FontWeight FontAngle Color
  BackgroundColor EdgeColor Margin HorizontalAlignment VerticalAlignment
  Rotation Interpreter LineStyle LineWidth Clipping`
* image: `CData XData YData CDataMapping`
* bar group: `XData YData BarWidth BaseValue FaceColor EdgeColor LineWidth
  LineStyle Horizontal BarLayout`; area: `FaceColor EdgeColor LineWidth
  LineStyle BaseValue XData YData`; stem/stairs: line properties (+ stem
  `BaseValue`); contour: `LevelList[Mode] LineColor LineStyle LineWidth Fill ZData`
* legend: `String Location`; colorbar: `Location Label Limits(get)`

Emitted records: `line` (also `stem`, `stairs`, bar baseline with `span`,
contour lines with `role:"contour"`), `scatter` (2-D and single-colour 3-D),
`surface` (flat/interp/constant/none colours, NaN cells, mesh styles, scalar
alpha), `patch2d` (fill/patch/bar/area, with `bar` data-tip records),
`patch3d` (`contourf` native rings), `text`, `image` (indexed/truecolor,
scaled/direct), colorbars, legends, every budget and rejection code that
applies to these objects (the contract's fallback is the only output for
unsupported content — never a partial scene).

## Measured against Octave (fltk) — evidence

`motor/scripts/grafik_oracle.sh` runs each case of
`tests/oracle/cases.txt` in a fresh `octave-cli` (fltk, invisible figure)
through `__mf_figure_data__` and stores the JSON; `tests/oracle.rs`
rebuilds the same figure through the Rust API and compares every field
(1e-9 relative; positions 1e-8). Result: **all 58 cases identical** (no
accepted differences; `KNOWN` in `tests/oracle.rs` is empty).
`motor/scripts/grafik_measure.sh` produced `tests/oracle/measure.json`
(ticks for 160+ ranges, subplot grids, colorbars, cameras, defaults,
geometry); `tests/oracle/colormaps.json` holds measured colormaps.

Rules fitted black-box to those measurements:

* **Tick step**: Lewart's SCALE1 (published, CACM 1973) with 5 intervals —
  step = {1,2,5,10}·10^k, thresholds √2, √10, √50 on `range/5/10^k`.
* **Automatic limits**: two passes — round the data outward to the step of
  its range, then round again to the step of the rounded range; ticks are
  the multiples `k·step` of the second step. No floating tolerance
  (`ceil(2e-5/2e-6)=11`). Constant data `v` expands to `v∓0.1|v|`, 0 to
  `[-1,1]`; no data: x/y `[0,1]`; an axes with children but no z: `[-1,1]`.
  Checked on 12514 axis measurements (sweeps in `measure.json`): 2 mismatches,
  both the same case (`plot(0.3)` y-limits,
  `[0.27,0.33]` vs Octave `[0.26,0.33]`, a last-bit difference in the
  degenerate expansion; not resolved).
* Fixed limits (manual, `axis tight`, image, stretched) use one pass.
  Images make x/y limits tight to the pixel edges.
* **Tick labels** `%.5g` for |v| ≥ 1, `%.4g` below: 2194/2195 measured
  labels; the miss is `100005` (Octave `1.0000e+05`). Log axes: decades enclosing the data, ticks `10^k`,
  labels TeX `10^{k}` (measured with semilogx/loglog).
* **CLim** auto: range of all colour data; equal ends → `c∓1`; none → `[0,1]`.
  Measured quirks reproduced: scatter CData counts even as an RGB triple
  (`scatter(...)` → `[0 0.741]`); bar/area/stem/stairs/RGB patches count as 0
  (→ `[-1 1]`).
* **DataAspectRatio** auto = `range./pbar` normalised by its minimum.
  `axis equal`: DAR `[1 1 1]` manual, PBAR = the axes pixel box `[w h h]`
  in 2-D (`[1 1 1]` in 3-D), automatic limits stretched about their centre.
* **Camera** (26 measured views incl. PBAR `[1 2 3]`, `axis equal`, view
  `[90 0]`, `[0 0]`, `[45 60]`, `[120 -20]`): with `dims = range./DAR`, the
  camera is `5·|dims|` from the box centre along
  `[sin az cos el, -cos az cos el, sin el]` (scaled back by DAR); the view
  angle is `2·atan(e/d)` with `e` the larger projected half extent of the
  box. Up vector `[0 0 1]`, or `[-sin az·xr/pbx, cos az·yr/pby, 0]` at
  `|el| = 90`.
* **Subplot**: gap `21.25 px/figure size + 0.2/n`, pitch `(extent+gap)/n`
  inside `[0.13 0.11 0.775 0.815]`; matches 1–4 rows/cols, spans, and a
  300×900 figure to ~3e-10 (the 21.25 px constant is fitted).
* **Colorbar**: `eastoutside` shrinks the peer to 0.8w and puts the bar at
  `x+0.9w`, width `0.06w`; the other 7 locations measured likewise; bar
  limits = CLim, ticks one-pass on CLim. Deleting it restores the position.
* **Bars**: n>1 grouped series, spacing `w(1+w)/(2n)·dx`, bar width
  `w(3w−1)/(2n)·dx`; single series `w·dx`; `dx` = smallest x spacing;
  stacked: positives and negatives stack separately. 17 measured layouts.
  `bar` sets the x ticks to the bar positions (manual mode).
* Area patches: base→data→base→back along the base. `stairs` and
  single-series `area` pull the limits to include `[0,1]` (y only when
  BaseValue is 0) — an Octave quirk, measured and reproduced.
* Octave keeps the zlabel text invisible (`plot3(...); zlabel('Z')` exports
  `""`); reproduced. surf/mesh/scatter3 switch the grid on, plot3 does not.
* **Contours** (`contour.rs`, own marching-squares tracer): default levels
  `unique(linspace(zmin,zmax,n+2)(1:n+1))` (n=10); contour draws the levels
  above zmin, contourf all, as native rings incl. background holes (NaN
  level). Crossing arithmetic, path start/order, saddle pairing, closed loops
  and the hole patch match Octave bit for bit on 10 measured cases (basic,
  ring, filled ring, saddle n=5, non-uniform X/Y, explicit levels, filled
  levels, filled and unfilled hole). Child order is reversed into drawing
  order in `scene.rs`. NaN data is not part of the parity (Octave emits
  degenerate dead-end paths); there cells are skipped, and the result still
  passes the gate. Curvilinear X/Y matrices are mapped with `interp2`
  arithmetic but were not measured end to end.
* Colour order (re-measured in lane r/grafik-3; the earlier note here was
  wrong): a LineSpec colour (`'r'`, `'k--'`) never advances the order, in
  one call or under `hold on`; a `'Color'` name/value does (it overrides the
  cycled colour afterwards); `line(...,'Color',c)` does not.
* Reversed x direction negates the camera's x term.
* Colours: colour order and all 11 colormaps at 14 sizes match the measured
  values (`tests/colormaps.rs`); `viridis` (default, 256 rows) is the
  measured table. `gray()` without N takes the current map's length.
* Defaults read from the oracle: line width 0.5, marker size 6, text margin
  3, legend `northeast`, hold continues the colour order, replotting
  without hold deletes the legend and resets labels.

## Assumed, not measured

* `tiledlayout`/`nexttile` do not exist in Octave 11.3: tiles reuse the
  subplot grid formula (MATLAB's real tile spacing differs).
* Reversal of y/z in the camera (only x was measured; applied by symmetry).
* Bar group with `FaceColor 'flat'`, area `BaseValue`≠0 geometry, horizontal
  stacked bars, errorbar/xline/yline/annotation (not implemented).
* Interactive pixel rendering is the viewer's job; the files and PNGs come
  from `indy-raster` (see the last section), measured against the viewer.

## fltk vs Qt

Qt also runs headless here (`octave --no-gui`, `QT_QPA_PLATFORM=offscreen`,
`GRAFIK_TOOLKIT=qt grafik_oracle.sh`). All 39 `uret.m` recipes and the first
50 cases gave **0 differing fields** between fltk and Qt with the current
extractor. The committed Qt fixtures in `tests/fixtures/figure_v3` differ only
in fields an older extractor did not emit (`pixel_count`, `limits.image_pixels`,
`source.figure_size`, per-axes tick modes). The 8 contour cases added later
were run under fltk only.

## Validation

* `cargo test -p indy-graphics` — unit tests, `tests/oracle.rs` (parity),
  `tests/colormaps.rs`, `tests/api.rs` (behaviour + a 400-session fuzz test:
  random calls never panic, always emit valid JSON), `tests/validate.rs`.
* `tests/validate.rs` and `motor/scripts/grafik_validate.sh` run the UI's
  own admission gate `frontend/figure_data_utils.cjs validate()` (node)
  over all 58 oracle cases, 26 extra cases (reduction, budgets, alpha, log,
  truecolor, every rejection path) and 150 random sessions: every figure
  must be `ok:true`, and each named case must be drawable or end in its
  expected reason code. Without node the test fails unless
  `INDY_GRAPHICS_SKIP_NODE=1` is set (explicit skip).

## Interpreter hook

Integration point: `SEAM(r/grafik)` in
`motor/crates/indy-kernel/src/session_builtins.rs` on main (not wired by this
lane). Each entry there is a `BuiltinFn = fn(&mut Interp, Vec<Value>, usize)
-> R<Vec<Value>>`. The integration needs:

1. A `Graphics` stored in the session state reachable from `Interp::session`.
2. A `Value → PropValue` adapter: real numeric/logical → `Array` (class
   kept in `NumClass`), char row → `Str`, cellstr → `Cell`; and back:
   handles → double (`Handle::value`), `Array` → numeric, `Str`/`Cell` →
   char/cellstr. `GraphicsError { id, message }` → `err(id, message)`.
3. One small function per builtin parsing positional arguments into the API
   types below (`callargs::split_series` covers plot/plot3/stairs/stem;
   `callargs::name_value` the trailing pairs).
4. After each `eval`, `take_changed()` and write each JSON as the figure
   artifact (with the job id/ordinal as `__mf_execute__` does), dropping
   closed figures.

Names in the seam table this crate covers: `plot plot3 figure subplot hold
axis axes xlabel ylabel zlabel title legend grid box close clf cla gcf gca
drawnow(no-op) surf mesh contour contourf bar barh scatter scatter3 line
patch fill area stem stairs semilogx semilogy loglog image imagesc colorbar
colormap caxis clim view xlim ylim zlim xticks yticks zticks xticklabels
yticklabels text` (the tick functions are `set(ax,'XTick'/'XTickLabel')`).
Not covered — they keep the explicit "graphics not yet available" error
(lane r/grafik-2 implemented the others, see the next section): `gco shg
refresh ezplot annotation light lighting camlight rotate3d zoom pan ginput
uicontrol uifigure waitbar linkaxes`; `print saveas exportgraphics` say that
saving figures to files is not available (no image renderer; no file is
ever written). These stub-only names are invisible to `exist` (0) and
`which` (`''`), so check-then-call scripts take their fallback branch; a
user `.m` file of the same name still counts. (`print saveas exportgraphics`
now work, see the last section.)

The interpreter keeps one `Graphics` per session and maps each MATLAB
graphics builtin to one call; after every statement the kernel calls
`take_changed()` and exports the returned figure JSON (`None` = closed).
`callargs::split_series` splits `plot`-style argument lists;
`callargs::name_value` pairs trailing options. Handles cross the boundary
as `Handle::value()`/`Handle::from_value` (f64).

```rust
// session
fn new() -> Graphics
fn figure(&mut self, n: Option<f64>) -> Result<Handle>
fn gcf(&mut self) -> Handle;  fn gca(&mut self) -> Handle
fn axes(&mut self, pos: Option<[f64; 4]>) -> Result<Handle>
fn make_current(&mut self, h: Handle) -> Result<()>        // figure(h), axes(h)
fn hold(&mut self, ax: Option<Handle>, on: Option<bool>) -> Result<()>
fn subplot(&mut self, m: usize, n: usize, p: &[usize]) -> Result<Handle>
fn close(&mut self, t: CloseTarget) -> Result<()>;  fn clf(&mut self, f: Option<Handle>) -> Result<()>
fn delete(&mut self, h: Handle) -> Result<()>
// plotting (ax: Option<Handle>, opts: &Opts)
fn plot / plot3 (&mut self, ax, &[PlotSeries], opts) -> Result<Vec<Handle>>
fn plot_log(&mut self, ax, &[PlotSeries], log_x, log_y, opts) -> Result<Vec<Handle>>
fn line(&mut self, ax, x, y, z: Option<&[f64]>, opts) -> Result<Handle>
fn scatter(&mut self, ax, &ScatterArgs, opts) -> Result<Handle>
fn bar(&mut self, ax, &BarArgs, horizontal: bool, opts) -> Result<Vec<Handle>>
fn stairs(&mut self, ax, &PlotSeries, opts) / stem(.., filled, opts) -> Result<Vec<Handle>>
fn area(&mut self, ax, x: Option<&[f64]>, y: &Array, opts) -> Result<Vec<Handle>>
fn fill / patch (&mut self, ax, x, y, color: &PropValue, opts) -> Result<Handle>
fn patch_faces(&mut self, ax, vertices: Array, faces: Array, opts) -> Result<Handle>
fn surf / mesh (&mut self, ax, &GridArgs, opts) -> Result<Handle>
fn contour(&mut self, ax, &GridArgs, &Levels, filled: bool, opts) -> Result<Handle>
fn image(&mut self, ax, x: Option<[f64;2]>, y: Option<[f64;2]>, c: Array, scaled: bool, opts) -> Result<Handle>
fn text(&mut self, ax, pos: [f64; 3], lines: &[String], opts) -> Result<Handle>
// annotation
fn label(&mut self, ax, LabelRole, lines: &[String], opts) -> Result<Handle>
fn legend(&mut self, ax, labels: Option<&[String]>, location: Option<&str>) -> Result<Handle>
fn grid(&mut self, ax, on: Option<bool>, minor: bool) -> Result<()>
fn axis(&mut self, ax, &AxisCmd) -> Result<()>
fn set_limits(&mut self, ax, axis: usize, lim: Option<[f64; 2]>) -> Result<()>  // xlim/ylim/zlim
fn limits(&mut self, ax, axis: usize) -> Result<[f64; 2]>                       // xlim() query
fn caxis(&mut self, ax, Option<[f64; 2]>) -> Result<()>
fn colormap(&mut self, ax, Option<&ColormapArg>) -> Result<Colormap>
fn colorbar(&mut self, ax, location: Option<&str>) -> Result<Handle>
fn view(&mut self, ax, [f64; 2]) -> Result<()>
// properties and export
fn set(&mut self, h: Handle, name: &str, v: PropValue) -> Result<()>
fn get(&self, h: Handle, name: &str) -> Result<PropValue>
fn figure_json(&self, fig: Handle) -> String
fn take_changed(&mut self) -> Vec<(Handle, Option<String>)>
```

Kernel side (not this lane): the export identity (`source.job` 32-hex id
and `source.figure` ordinal) is set by the exporter, exactly as
`__mf_execute__` does for Octave; `figure_json` emits the direct-extraction
default `{"job":"","figure":0}`. The PNG the contract assumes for
`supported:false` comes from `indy-raster`.

## Frontend notes (no frontend edits made)

* `supported:false` records are valid (`ok:true`) and the UI shows the PNG
  next to them. Only if the renderer itself failed is there no image; the UI
  then shows the localized `reason_code`.

## Lane r/grafik-2 (30 Oct 2026): histogram and the missing functions

Code: `hist.rs`, `figtext.rs` (sgtitle), `pie.rs`, `yy.rs`, `lines2.rs`
(errorbar, quiver, fplot, polar), `surf2.rs` (shading, imshow, contour3,
surfc, meshc, bar3); builtins in `indy-graphics-builtins/src/charts.rs`,
`lane_a.rs`, `lane_b.rs`. Tests: `indy-graphics-builtins/tests/r2.rs`,
`lane_a.rs`, `lane_b.rs`, `oracle_r2.rs`; every new figure goes through the
UI gate. `examples/grafik_run.rs` runs a script headless and writes the
figure JSON (`cargo run -p indy-graphics-builtins --example grafik_run --
file.m out/`).

New oracle sets (same field-by-field comparison as the 58 cases):
`tests/oracle_r2/` (8 `hist` cases, Octave native), `tests/oracle_compat/`
(5 `sgtitle` cases, measured with IndyMAT's own `octave/compat` on the path:
`GRAFIK_COMPAT=1 GRAFIK_CASES=... grafik_oracle.sh`), `tests/lane_b/`
(18 contour3/surfc/meshc/shading/imshow cases). Accepted differences are
listed in the tests (`KNOWN`), each with its reason.

| Function | Representation | Evidence |
|---|---|---|
| `histogram` | `patch2d` role `bar` (no baseline), FaceAlpha 0.6 carried as `face_alpha`; stairs style = `line` role `histogram` | Bins from the interpreter's `histcounts` (so `Values`/`BinEdges` equal `histcounts`); MATLAB documented, not measured (Octave has none) |
| `hist` | one patch per column, `FaceColor flat`, CData = column | Octave measured: `(a,b]` bins, constant data, ±Inf, matrices (0.64 group width), integer centres → manual ticks |
| `sgtitle` | invisible overlay axes `[0 0 1 1]` drawn last, `textbox` patch + text with `figure_title` | Identical to IndyMAT's compat sgtitle in Octave (5 cases): reserve `1.2·FontSize·lines/H + 2·6/H`, subplot axes shrink |
| `pie` | patches + texts; record `position` = the drawn square box so slices stay round | Octave measured (point counts `ceil(90f)+1`, labels, explode); position is a documented approximation |
| `yyaxis` | two physical axes, side colours | No right ruler in the contract → explicit `unsupported_object {type:"yyaxis"}` |
| `errorbar` | two `line` records role `errorbar` | Octave structure; caps: CapSize points → data units at export (approximation) |
| `quiver` | `line` records role `quiver` | Autoscale and heads fitted to Octave |
| `fplot` | line, adaptive sampling, NaN at poles | Own rule (Octave's sampling differs) |
| `polarplot`/`polar` | hidden equal-aspect cartesian axes, `polar_grid` lines, text labels | Approximation, not a polaraxes |
| `shading`, `imshow`, `contour3`, `surfc`, `meshc` | surface/image/3-D contour lines | Octave measured (lane_b cases) |
| `bar3` | one Surface per column (NaN-separated boxes) | MATLAB documented, not measured |

Also fixed: `subplot` gaps use the figure's own pixel size (the measured
formula already took it; the call passed 560×420), and `bar` sets manual x
ticks only for integer positions (`bar([1.5 2.5],…)` keeps auto, measured).

## Lane r/grafik-3 (30 Oct 2026): parity from the visual measurement

Driven by the screenshot comparison of the example scripts
(`~/.cache/indymat-motor-fleet/gorsel/`). Measured black-box in Octave 11.3;
new oracle set `indy-graphics-builtins/tests/oracle_g3/` (42 cases, through
the interpreter, field by field) and `indy-graphics/tests/oracle/colormaps2.json`.

* **Colour order** (see above): LineSpec colours do not advance it; a
  `'Color'` name/value does; `line(...,'Color',c)` does not. `quiver`
  consumes 3–4 entries in Octave (its hidden lines); not reproduced.
* **Name/value pairs in a multi-series `plot`** apply to every line. Octave
  applies them to the last x/y group only (`plot(t,a,'b',t,b,'r','LineWidth',2)`
  gives the first line 0.5); MATLAB's documentation says "The properties
  apply to all the plotted lines". MATLAB is followed (deliberate difference;
  visible in `temel_deneme`/`doyum_antiwindup` as one line width).
* **stem**: marker colours are fixed at creation to the line colour; a
  later `'Color'` (name/value or `set`) recolours only the stems, as Octave.
* **Subplot insets** (`inset.rs`): an axes needs `10 px + title` above,
  `16.51 px + xlabel` below and `17.5 px + ylabel` left (tick-label part
  fixed at the default font size, measured from widths otherwise); the outer
  box of a cell ends half the 21.25 px gap beyond the neighbour; the largest
  overflow on each side is taken from every subplot axes of the figure.
  Text heights: fitted ink-box model in half pixels. Measured with
  `GRAFIK_TOOLKIT=qtnative` (new option of `grafik_oracle.sh`): the IndyMAT
  app runs `octave --no-gui` on the native Qt platform, whose text extents
  are ~1 px smaller than fltk/offscreen Qt (`diferansiyel_denklem`: 0.3075
  native, 0.3051 fltk). `get(ax,'Position')` reports the shrunk position.
  Accepted in `oracle_g3.rs`: ≤0.4 px on multi-line labels (sub-pixel ink).
* **Colorbar next to a fixed-aspect 2-D box** (`axis equal/image`,
  `daspect`, `pbaspect`): an outside colorbar spans the drawn plot box (PBAR
  x:y fitted in the peer, centred) and keeps its gap to that box. 3-D
  (`surf; axis equal; colorbar`) projects differently; not done.
* **Camera view angle**: the angle spans the horizontal half extent of the
  projected box when `h_half ≥ v_half · (width/height of the axes in
  pixels)`, else the vertical one (180 measured views × viewports × PBARs,
  all exact). Fixes 2-D `axis equal` and narrow 3-D subplots.
* **`axis image`** keeps the plot box automatic from the tight ranges
  (PBAR `[1.5 5 1]` for a 10×3 image), not `axis equal`'s pixel box;
  `daspect`/`pbaspect` added.
* **Colormaps** `turbo hsv pink lines colorcube prism flag white cubehelix
  ocean rainbow` (all Octave 11.3 has; it lacks `parula sky abyss`): 153
  measured maps match to ≤1e-15 (`turbo` is the measured 256-row table).
* **UI** (`frontend/figures.js`, `static/style.css`): 2-D line widths are
  points (96/72 CSS px at the figure's on-screen scale, ≥1 device px); the
  panel's Reset/Fullscreen overlay shows on hover/focus so it no longer hides
  the right-hand axes title.
* **Handles** (`indy-graphics-builtins/src/handles.rs`): `isfigure`,
  `setappdata getappdata isappdata rmappdata`, `ancestor`, `allchild`
  (semantics checked against octave-cli), `UserData` of any value, figure
  `DefaultAxesColorOrder`, `Units` (reported; other units refused), `gco`
  (always `[]`: figures take no clicks). Dot access `h.Prop`, `h.Prop = v`,
  `ax.XLim(2) = v`, `h.Parent.Type` on live handles through the additive
  interpreter hook `Interp::handle_field_get/set` (None unless graphics is
  registered; consulted only where dot indexing on a number would error).
  Limits: a plain double equal to an open figure's number is that figure
  (`x = 1; x.Name = 'a'` renames figure 1); `ax.XLim(end) = v` is not
  supported (reading `ax.XLim(end)` is). The compat `yyaxis.m` still needs
  listeners/`DeleteFcn` and `exportgraphics` ends in `print` (refused).

## Lane r/raster (30 Oct 2026): `indy-raster`, figures to pixels and files

Crate `motor/crates/indy-raster`, pure Rust, no external crates, no system
fonts. It draws a **Figure-Data record**, not the model: its painters
(`paint2d`, `paint3d`) are ports of `frontend/figures.js`, `figure3d.js`,
`figure_webgl.js` and their utils, so one record gives one picture, saved JSON
can be re-rendered for measurement, and layout lives in one place.

* `Graphics::raster_record(fig)` is `figure_record` with the contract's budgets
  and refusals lifted (thread-local switch in `emit.rs`; the wire records are
  unchanged, the oracle sets prove it). Beyond work bounds (1M surface vertices,
  8M image pixels, 200k line samples) it still refuses, and the PNG says why.
  Extra fields: `yaxis_location`, `axes_color(_none)`, `point_colors`,
  `point_edge/face`, `patch_raster` records, `z` on text. Small accessor:
  `Graphics::record_axes`.
* Devices: `raster` (exact-coverage scanline fill, non-zero and even-odd;
  strokes with miter joins and dashes; sRGB blending), `vector` (SVG, PDF with
  text as outlines; a 3-D scene is an embedded image). `paint3d` keeps a
  4-sample depth buffer per axes, like the UI's multisampled canvas;
  transparent triangles draw far to near (the UI peels depth).
* `encode`: PNG over `indy_value::deflate` (from `r/dosya`, same commit),
  baseline JPEG, TIFF.
* Fonts: `fonts/README.md`. Liberation Sans reproduces the measured width of
  the UI's sans at every sample; JetBrains Mono is the UI's tick font.
* Themes: `Theme::paper` (white, colours untouched) for every file and session
  PNG; `Theme::ui_dark/ui_light` (3:1 contrast rule) for measurement.
* Deliberate differences from the 2-D viewer: marker size follows
  `MarkerSize`/`SizeData` (the viewer has one fixed size, which is what the
  default size draws), legend honours `Location`, TeX also in 3-D labels.
* Builtins (`indy-graphics-builtins/src/export.rs`): `print` (`-d`png, jpeg[NN],
  tiff, pdf, svg; `-r`; `-f`; `-RGBImage`), `saveas`, `exportgraphics`
  (`Resolution`, `ContentType`, `BackgroundColor`; an axes target is cropped to
  its content). Everything else is an explicit `IndyR:exportUnsupported`.
* Kernel: `open_figures` renders changed figures only, one thread each, at
  120 dpi; `png_base64` carries it and `backend/kernel_rust.py` stores
  `figure-N.png` as before.
* Known limit: with `octave/compat` on the path (`skor.py tam`) the compat
  `exportgraphics.m` shadows the builtin and needs Octave internals; the app
  does not put compat on its path.
