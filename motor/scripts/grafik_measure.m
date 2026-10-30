function grafik_measure(out_file, only)
  % Black-box behaviour measurements of Octave (fltk) graphics for indy-graphics.
  % Only get() on real objects created through public functions; no source reading.
  % Usage (via grafik_measure.sh): grafik_measure('/path/measure.json')
  here = fileparts(mfilename('fullpath'));
  root = fileparts(fileparts(here));
  ws = warning(); warning('off', 'all');
  addpath(fullfile(root, 'octave'));
  graphics_toolkit('fltk');
  warning(ws); warning('off', 'Octave:shadowed-function');

  M = struct();
  M.meta = struct('octave_version', OCTAVE_VERSION, 'toolkit', 'fltk', ...
    'default_figure_position', {vec(get(groot, 'defaultfigureposition'))}, ...
    'screensize', {vec(get(groot, 'screensize'))}, ...
    'exist', struct('tiledlayout', exist('tiledlayout'), 'nexttile', exist('nexttile'), ...
      'parula', exist('parula'), 'viridis', exist('viridis'), 'turbo', exist('turbo')));
  sections = {'ticks', 'ticks_sweep', 'ticks_fine', 'ticks_size', 'tick_labels', 'axis_modes', 'subplot', ...
    'colorbar', 'camera', 'colors', 'defaults', 'geometry', 'clim'};
  if nargin > 1 && ~isempty(only), sections = only; end
  for i = 1:numel(sections)
    s = sections{i};
    try
      M.(s) = feval(['m_' s]);
    catch err
      M.(s) = struct('error', err.message);
      fprintf(2, 'section %s failed: %s\n', s, err.message);
    end
    close all;
  end
  fid = fopen(out_file, 'w'); fputs(fid, jsonencode(M)); fputs(fid, "\n"); fclose(fid);
  printf('wrote %s\n', out_file);
end

% ---------------------------------------------------------------- helpers
function f = nf(varargin)
  close all;
  f = figure('visible', 'off', varargin{:});
end
function c = vec(x)
  if iscell(x), c = x(:).'; return; end
  c = num2cell(double(x(:).'));
end
function c = mat(x)
  % Matrix as list of rows (row-major), robust for 1xN / Nx1 / empty.
  x = double(x); c = cell(1, rows(x));
  for r = 1:rows(x), c{r} = num2cell(x(r, :)); end
end
function c = lbl(x)
  if ischar(x), x = cellstr(x); end
  c = x(:).';
end
function s = axinfo(ax)
  s = struct();
  for d = 'xyz'
    s.([d 'lim']) = vec(get(ax, [d 'lim']));
    s.([d 'limmode']) = get(ax, [d 'limmode']);
    s.([d 'tick']) = vec(get(ax, [d 'tick']));
    s.([d 'ticklabel']) = lbl(get(ax, [d 'ticklabel']));
  end
  s.position = vec(get(ax, 'position'));
  s.outerposition = vec(get(ax, 'outerposition'));
  s.tightinset = vec(get(ax, 'tightinset'));
  fp = get(ancestor(ax, 'figure'), 'position');
  p = get(ax, 'position');
  s.pixel_size = vec([p(3)*fp(3) p(4)*fp(4)]);
end
function s = tickrec(ax, d)
  s = struct('lim', {vec(get(ax, [d 'lim']))}, 'tick', {vec(get(ax, [d 'tick']))}, ...
    'label', {lbl(get(ax, [d 'ticklabel']))});
end
function s = cam(ax)
  s = struct('cameraposition', {vec(get(ax, 'cameraposition'))}, ...
    'cameratarget', {vec(get(ax, 'cameratarget'))}, ...
    'cameraupvector', {vec(get(ax, 'cameraupvector'))}, ...
    'cameraviewangle', get(ax, 'cameraviewangle'), ...
    'dataaspectratio', {vec(get(ax, 'dataaspectratio'))}, ...
    'dataaspectratiomode', get(ax, 'dataaspectratiomode'), ...
    'plotboxaspectratio', {vec(get(ax, 'plotboxaspectratio'))}, ...
    'plotboxaspectratiomode', get(ax, 'plotboxaspectratiomode'), ...
    'view', {vec(get(ax, 'view'))}, 'projection', get(ax, 'projection'), ...
    'xlim', {vec(get(ax, 'xlim'))}, 'ylim', {vec(get(ax, 'ylim'))}, 'zlim', {vec(get(ax, 'zlim'))}, ...
    'position', {vec(get(ax, 'position'))});
end
function p = mf_positions(f)
  d = __mf_figure_data__(f);
  p = {};
  for k = 1:numel(d.axes)
    a = d.axes{k};
    q = struct('position', {vec(a.position)});
    if isfield(a, 'colorbars') && ~isempty(a.colorbars)
      cbs = a.colorbars; if ~iscell(cbs), cbs = num2cell(cbs); end
      q.colorbars = cbs;
    end
    p{end+1} = q;
  end
end
function s = patchinfo(h)
  s = struct('vertices', {mat(get(h, 'vertices'))}, 'faces', {mat(get(h, 'faces'))}, ...
    'xdata', {mat(get(h, 'xdata'))}, 'ydata', {mat(get(h, 'ydata'))}, ...
    'facevertexcdata', {mat(get(h, 'facevertexcdata'))}, 'cdata', {mat(get(h, 'cdata'))}, ...
    'facecolor', {colorval(get(h, 'facecolor'))}, 'edgecolor', {colorval(get(h, 'edgecolor'))}, ...
    'parent_type', get(get(h, 'parent'), 'type'));
end
function c = colorval(x)
  if ischar(x), c = x; else c = double(x(:).'); end   % 1x3 numeric -> JSON array
end
function s = lineinfo(h)
  s = struct('xdata', {vec(get(h, 'xdata'))}, 'ydata', {vec(get(h, 'ydata'))}, ...
    'zdata', {vec(get(h, 'zdata'))}, 'linestyle', get(h, 'linestyle'), 'marker', get(h, 'marker'), ...
    'color', {colorval(get(h, 'color'))}, 'markerfacecolor', {colorval(get(h, 'markerfacecolor'))}, ...
    'markeredgecolor', {colorval(get(h, 'markeredgecolor'))}, 'markersize', get(h, 'markersize'), ...
    'linewidth', get(h, 'linewidth'), 'parent_type', get(get(h, 'parent'), 'type'));
end
function c = children_desc(h)
  k = allchild(h); c = {};
  for i = 1:numel(k)
    c{end+1} = struct('type', get(k(i), 'type'), 'handlevisibility', get(k(i), 'handlevisibility'));
  end
end

% ---------------------------------------------------------------- a. ticks
function P = tick_pairs()
  P = [1e-5 2e-5; 0 1e-5; 1e-5 3.7e-5; 0.001 0.004; 0.001 0.0123; 0.01 0.02; 0.1 0.2;
       0 0.1; 0 0.3; 0.3 0.7; 0.3 7.7; 0 0.5; 0.25 0.75; 0 1; 0.5 1.5; 1 2; 1 3; 1 4; 1 5;
       1 6; 1 7; 1 8; 1 9; 1 10; 1 11; 1 12; 0 13; 0 17; 0 20; 0 25; 0 30; 0 40; 0 45;
       0 50; 0 60; 0 70; 0 75; 0 80; 0 90; 0 100; 1 100; -13 101; 3 97; 0 150; 0 200; 0 250;
       0 300; 0 350; 0 400; 0 500; 0 700; 0 1000; 1 1000; 0 1500; 0 2000; 0 5000; 0 1e4;
       1e4 2e4; 0 3.3e4; 0 1e5; 0 1e6; 1e5 1e7; 0 1e7; 123456 123999; 1e6 1.0001e6;
       -1 0; -5 -1; -100 -10; -1 1; -3 7; -0.5 0.5; -7.7 0.3; -1e4 1e4; -2 3;
       5 5; 0 0; -3 -3; 1e-5 1e-5; 1000 1000; 0.3 0.3; pi 2*pi; exp(1) 10; 0.05 0.95;
       2 2.1; 2 2.01; 10 10.5; 99 101; 1 1.1; 0 0.0007; 7 8; 0 3.5; 0 4.5; 0 6; 0 7; 0 9; 0 11];
end
function out = m_ticks()
  P = tick_pairs();
  rand('twister', 20261029);
  R = zeros(60, 2);
  for k = 1:60
    e = floor(rand()*12) - 5; a = (rand()*2 - 1) * 10^e; w = rand() * 10^(e + floor(rand()*3) - 1);
    R(k, :) = [a a + w];
  end
  P = [P; R];
  out = {};
  for k = 1:rows(P)
    a = P(k, 1); b = P(k, 2);
    f = nf();
    if a == b
      plot(a, b, 'o');  % single point (same for equal endpoints)
      kind = 'single_point';
    else
      plot([a b], [a b]);
      kind = 'pair';
    end
    ax = gca;
    out{end+1} = struct('data', {vec([a b])}, 'kind', kind, 'y', tickrec(ax, 'y'), 'x', tickrec(ax, 'x'), ...
      'pixel_size', {axinfo(ax).pixel_size});
  end
  % Equal endpoints as a 2-point line (plot([5 5])), distinct from a single point.
  for v = [5 0 -3 1e-5 1000]
    f = nf(); plot([1 2], [v v]); ax = gca;
    out{end+1} = struct('data', {vec([v v])}, 'kind', 'flat_line_x12', 'y', tickrec(ax, 'y'), 'x', tickrec(ax, 'x'), ...
      'pixel_size', {axinfo(ax).pixel_size});
  end
end

function out = m_ticks_sweep()
  % Dense sweep: data [a, a+r]*10^k plotted as y (and x) data. Compact rows:
  % [lo hi ylim1 ylim2 ytick_step ytick_count xlim1 xlim2 xtick_step xtick_count].
  % Tick step = ytick(2)-ytick(1); full tick vectors are in the 'ticks' section.
  f = nf(); rowsv = [];
  for k = [-3 0 2]
    for a = [0 0.3 0.5 1 -0.5 -1.5 -0.25 7]
      for r = 0.5:0.05:10
        lo = a * 10^k; hi = (a + r) * 10^k;
        clf(f); plot([lo hi], [lo hi]); ax = gca;
        yl = get(ax, 'ylim'); yt = get(ax, 'ytick'); xl = get(ax, 'xlim'); xt = get(ax, 'xtick');
        rowsv(end+1, :) = [lo hi yl yt(2)-yt(1) numel(yt) xl xt(2)-xt(1) numel(xt)];
      end
    end
  end
  out = struct('columns', {{'lo', 'hi', 'ylim1', 'ylim2', 'ystep', 'ycount', 'xlim1', 'xlim2', 'xstep', 'xcount'}}, ...
    'rows', {mat(rowsv)});
end

function out = m_ticks_fine()
  % Fine sweep (0.01 data steps) for several offsets; y data only, x fixed [0 1].
  % rows: [lo hi ylim1 ylim2 ystep ycount]
  f = nf(); rowsv = [];
  for lo = [0 0.5 -0.25 0.3 -1.5 7 0.37 -0.73]
    for r = 0.5:0.01:10
      hi = lo + r;
      clf(f); plot([0 1], [lo hi]); ax = gca;
      yl = get(ax, 'ylim'); yt = get(ax, 'ytick');
      rowsv(end+1, :) = [lo hi yl yt(2)-yt(1) numel(yt)];
    end
  end
  out = struct('columns', {{'lo', 'hi', 'ylim1', 'ylim2', 'ystep', 'ycount'}}, 'rows', {mat(rowsv)});
end

function out = m_ticks_size()
  P = [0 1; 0.3 7.7; -13 101; 1 1000; 0 13; 0 100; 1e5 1e7; 0.001 0.004; 0 7; 0 45; 1 10];
  setups = {'default', 'subplot221', 'subplot411', 'subplot141', 'fig300x200', 'fig1200x900', 'fig200x800', 'fig1600x300'};
  out = {};
  for k = 1:rows(P)
    for s = 1:numel(setups)
      su = setups{s};
      switch su
        case 'default', f = nf(); ax = axes();
        case 'subplot221', f = nf(); ax = subplot(2, 2, 1);
        case 'subplot411', f = nf(); ax = subplot(4, 1, 1);
        case 'subplot141', f = nf(); ax = subplot(1, 4, 1);
        case 'fig300x200', f = nf('position', [0 0 300 200]); ax = axes();
        case 'fig1200x900', f = nf('position', [0 0 1200 900]); ax = axes();
        case 'fig200x800', f = nf('position', [0 0 200 800]); ax = axes();
        case 'fig1600x300', f = nf('position', [0 0 1600 300]); ax = axes();
      end
      plot(ax, P(k, :), P(k, :));
      ai = axinfo(ax);
      out{end+1} = struct('data', {vec(P(k, :))}, 'setup', su, 'figure_position', {vec(get(f, 'position'))}, ...
        'pixel_size', {ai.pixel_size}, 'y', tickrec(ax, 'y'), 'x', tickrec(ax, 'x'));
    end
  end
  % Does a resize after plotting retrigger tick computation?
  f = nf(); plot([0 13], [0 13]); before = tickrec(gca, 'y'); set(f, 'position', [0 0 200 800]);
  after = tickrec(gca, 'y');
  out{end+1} = struct('setup', 'resize_after_plot_200x800', 'data', {vec([0 13])}, 'y_before', before, 'y_after', after);
end

% ---------------------------------------------------------------- b. tick labels
function out = m_tick_labels()
  P = [0 1e6; 1e5 1e7; 0 1e-5; 123456 123999; 1e10 2e10; 0 1e15; 1e-10 3e-10; 0 0.0007;
       -1e6 1e6; 0.1 0.7; 0 99999; 0 100000; 0 1e5+1; 99990 100010; 1e-4 1e-3; 0.12345 0.12349];
  out = {};
  for k = 1:rows(P)
    f = nf(); plot([0 1], P(k, :)); ax = gca;
    t = findall(f, 'type', 'text'); strs = {};
    for i = 1:numel(t)
      st = get(t(i), 'string'); if iscell(st), st = strjoin(st, '|'); end
      if ~isempty(st), strs{end+1} = struct('string', st, 'visible', get(t(i), 'visible'), ...
          'position', {vec(get(t(i), 'position'))}, 'units', get(t(i), 'units')); end
    end
    ytl = get(ax, 'yticklabel');
    out{end+1} = struct('data', {vec(P(k, :))}, 'y', tickrec(ax, 'y'), 'yticklabel_class', class(ytl), ...
      'yticklabelmode', get(ax, 'yticklabelmode'), 'texts_in_figure', {strs}, ...
      'num_text_objects_findall', numel(t));
  end
  % Manual ticks: does the label follow %g-like formatting?
  f = nf(); plot([0 1], [0 1]); ax = gca;
  set(ax, 'ytick', [0 0.123456789 0.5 1/3 1e-7 12345678 1.5e9]);
  out{end+1} = struct('manual_ytick', {vec(get(ax, 'ytick'))}, 'label', {lbl(get(ax, 'yticklabel'))});
  set(ax, 'ytick', [-2.5 0 1e5 123456 1234567 0.0001 0.00001]);
  out{end+1} = struct('manual_ytick', {vec(get(ax, 'ytick'))}, 'label', {lbl(get(ax, 'yticklabel'))});
  % Axes property names that could hold an exponent/multiplier
  p = fieldnames(get(ax));
  out{end+1} = struct('axes_props_matching_exp_ruler_mult', {p(~cellfun(@isempty, regexpi(p, 'exp|ruler|mult|offset|scale'))).'});
end

% ---------------------------------------------------------------- c. axis modes
function s = axstate(ax)
  s = cam(ax);
  s.xlimmode = get(ax, 'xlimmode'); s.ylimmode = get(ax, 'ylimmode'); s.zlimmode = get(ax, 'zlimmode');
  s.visible = get(ax, 'visible'); s.box = get(ax, 'box');
  s.xtick = vec(get(ax, 'xtick')); s.ytick = vec(get(ax, 'ytick'));
  s.xcolor = colorval(get(ax, 'xcolor')); s.color = colorval(get(ax, 'color'));
end
function out = m_axis_modes()
  out = struct();
  cmds = {'none', 'tight', 'equal', 'off', 'image', 'square', 'normal', 'equal_tight', 'auto_after_tight', 'manual', 'fill', 'vis3d'};
  for i = 1:numel(cmds)
    c = cmds{i};
    f = nf(); plot([0 3], [0 1]); ax = gca;
    try
      switch c
        case 'none'
        case 'equal_tight', axis equal; axis tight;
        case 'auto_after_tight', axis tight; axis auto;
        otherwise, axis(c);
      end
      out.(['line_' c]) = axstate(ax);
    catch err
      out.(['line_' c]) = struct('error', err.message);
    end
  end
  % axis tight on sin, and on image data
  f = nf(); x = linspace(0, 7, 30); plot(x, sin(x)); axis tight; out.sin_tight = axstate(gca);
  f = nf(); imagesc([1 2 3; 4 5 6]); out.imagesc_default = axstate(gca);
  f = nf(); imagesc([1 2 3; 4 5 6]); axis image; out.imagesc_image = axstate(gca);
  f = nf(); imagesc([1 2 3; 4 5 6]); axis equal; out.imagesc_equal = axstate(gca);
  f = nf(); surf([1 2; 3 4]); axis equal; out.surf_equal = axstate(gca);
  f = nf(); surf([1 2; 3 4]); axis tight; out.surf_tight = axstate(gca);
  f = nf(); plot([0 3], [0 1]); axis off; d = __mf_figure_data__(f);
  out.line_off_extractor = struct('axes_visible_fields', {fieldnames(d.axes{1}).'});
end

% ---------------------------------------------------------------- d. subplot
function out = m_subplot()
  out = struct();
  grids = [1 1; 1 2; 2 1; 2 2; 3 1; 1 3; 3 3; 2 3; 4 1; 1 4; 4 4];
  out.grids = {};
  for g = 1:rows(grids)
    m = grids(g, 1); n = grids(g, 2);
    for withplot = [false true]
      f = nf(); hs = zeros(1, m*n);
      for k = 1:m*n
        hs(k) = subplot(m, n, k);
        if withplot, plot(1:3); end
      end
      pos = {}; op = {};
      for k = 1:m*n, pos{k} = vec(get(hs(k), 'position')); op{k} = vec(get(hs(k), 'outerposition')); end
      e = struct('m', m, 'n', n, 'with_plot', withplot, 'position', {pos}, 'outerposition', {op});
      if withplot, e.extractor = mf_positions(f); end
      out.grids{end+1} = e;
    end
  end
  % spans
  sp = {{2, 2, [1 2]}, {2, 2, [1 3]}, {2, 2, [2 4]}, {2, 2, [1 4]}, {3, 3, [1 5]}, {3, 3, [4 6]}, {2, 3, [4 6]}};
  out.spans = {};
  for i = 1:numel(sp)
    a = sp{i}; f = nf(); h = subplot(a{:}); plot(1:3);
    out.spans{end+1} = struct('m', a{1}, 'n', a{2}, 'p', {vec(a{3})}, 'position', {vec(get(h, 'position'))}, ...
      'outerposition', {vec(get(h, 'outerposition'))}, 'extractor', {mf_positions(f)});
  end
  % labels/titles: do they change subplot position?
  f = nf(); h = subplot(2, 2, 1); plot(1:3); p0 = vec(get(h, 'position'));
  title('T'); xlabel('X'); ylabel('Y'); p1 = vec(get(h, 'position'));
  out.labels_effect = struct('before', {p0}, 'after_title_xlabel_ylabel', {p1});
  % figure size dependence of subplot positions
  f = nf('position', [0 0 300 900]); h = subplot(2, 2, 1); plot(1:3);
  out.fig300x900_221 = vec(get(h, 'position'));
  f = nf(); h = axes(); out.default_axes = struct('position', {vec(get(h, 'position'))}, 'outerposition', {vec(get(h, 'outerposition'))});
  out.tiledlayout_exists = exist('tiledlayout');
end

% ---------------------------------------------------------------- e. colorbar
function s = cbinfo(cb, ax)
  s = struct('cb_position', {vec(get(cb, 'position'))}, 'ax_position', {vec(get(ax, 'position'))}, ...
    'cb_ylim', {vec(get(cb, 'ylim'))}, 'cb_ytick', {vec(get(cb, 'ytick'))}, 'cb_yticklabel', {lbl(get(cb, 'yticklabel'))}, ...
    'cb_xlim', {vec(get(cb, 'xlim'))}, 'cb_xtick', {vec(get(cb, 'xtick'))}, 'cb_xticklabel', {lbl(get(cb, 'xticklabel'))}, ...
    'cb_location', get(cb, 'location'), 'cb_type', get(cb, 'type'), 'cb_tag', get(cb, 'tag'), ...
    'cb_yaxislocation', get(cb, 'yaxislocation'), 'cb_xaxislocation', get(cb, 'xaxislocation'), ...
    'clim', {vec(get(ax, 'clim'))});
end
function out = m_colorbar()
  out = struct();
  f = nf(); ax = axes(); p0 = vec(get(ax, 'position')); surf([1 3 5; 2 4 6]); cb = colorbar();
  out.default = cbinfo(cb, ax); out.default.ax_position_before = p0; out.default.extractor = mf_positions(f);
  clims = [1 6; 0 1; -3 7; 0 255; 0 0.01; -1e4 1e4];
  out.clims = {};
  for i = 1:rows(clims)
    f = nf(); imagesc([1 2; 3 4]); ax = gca; caxis(clims(i, :)); cb = colorbar();
    a = cbinfo(cb, ax); a.order = 'caxis_then_colorbar'; out.clims{end+1} = a;
    f = nf(); imagesc([1 2; 3 4]); ax = gca; cb = colorbar(); caxis(clims(i, :));
    a = cbinfo(cb, ax); a.order = 'colorbar_then_caxis'; out.clims{end+1} = a;
  end
  locs = {'eastoutside', 'westoutside', 'northoutside', 'southoutside', 'east', 'west', 'north', 'south'};
  out.locations = {};
  for i = 1:numel(locs)
    try
      f = nf(); surf([1 3 5; 2 4 6]); ax = gca; cb = colorbar('location', locs{i});
      a = cbinfo(cb, ax); a.requested = locs{i}; out.locations{end+1} = a;
    catch err
      out.locations{end+1} = struct('requested', locs{i}, 'error', err.message);
    end
  end
  out.subplot = {};
  for k = 1:4
    f = nf(); for j = 1:4, h(j) = subplot(2, 2, j); imagesc(magic(3)); end
    p0 = vec(get(h(k), 'position')); cb = colorbar(h(k));
    a = cbinfo(cb, h(k)); a.subplot_index = k; a.ax_position_before = p0; out.subplot{end+1} = a;
  end
  f = nf(); h = subplot(2, 1, 2); plot(1:3); p0 = vec(get(h, 'position')); cb = colorbar();
  a = cbinfo(cb, h); a.ax_position_before = p0; out.subplot_212_line = a;
  % deleting the colorbar: does the axes grow back?
  f = nf(); imagesc(magic(3)); ax = gca; p0 = vec(get(ax, 'position')); cb = colorbar(); p1 = vec(get(ax, 'position'));
  delete(cb); out.delete_restores = struct('before', {p0}, 'with_cb', {p1}, 'after_delete', {vec(get(ax, 'position'))});
  % figure size dependence
  f = nf('position', [0 0 1200 900]); imagesc(magic(3)); ax = gca; cb = colorbar(); out.fig1200x900 = cbinfo(cb, ax);
  f = nf('position', [0 0 300 200]); imagesc(magic(3)); ax = gca; cb = colorbar(); out.fig300x200 = cbinfo(cb, ax);
  % colorbar with plain line (clim [0 1])
  f = nf(); plot(1:3); ax = gca; cb = colorbar(); out.line = cbinfo(cb, ax);
end

% ---------------------------------------------------------------- f. camera
function out = m_camera()
  out = struct();
  f = nf(); plot([1 2 4], [3 5 4]); out.line2d = cam(gca);
  f = nf(); plot([0 10], [0 1]); out.line2d_wide = cam(gca);
  f = nf(); plot([0 1], [0 100]); out.line2d_tall = cam(gca);
  f = nf(); plot([0 1], [0 1]); out.line2d_unit = cam(gca);
  f = nf(); plot([-5 5], [20 30]); out.line2d_offset = cam(gca);
  f = nf('position', [0 0 1200 300]); plot([0 1], [0 1]); out.line2d_fig1200x300 = cam(gca);
  f = nf(); [X, Y] = meshgrid(1:3, 1:2); surf([1 3 5; 2 4 6]); out.surf_default = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(45, 60); out.surf_view45_60 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(90, 0); out.surf_view90_0 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(0, 0); out.surf_view0_0 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(0, 90); out.surf_view0_90 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(-37.5, 30); axis equal; out.surf_view_m37_30_equal = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(-37.5, 30); out.surf_view_m37_30 = cam(gca);
  f = nf(); surf(peaks(10)); out.surf_peaks10 = cam(gca);
  f = nf(); surf(peaks(10)); view(120, -20); out.surf_peaks10_view120_m20 = cam(gca);
  f = nf(); plot3([0 1 2], [0 1 0], [0 0 1]); view(3); out.plot3_view3 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(30, -90); out.surf_view30_m90 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); view(30, 90); out.surf_view30_90 = cam(gca);
  f = nf(); plot3([0 1 2], [0 1 0], [0 0 1]); out.plot3_default = cam(gca);
  f = nf(); plot3([0 10], [0 100], [0 1000]); out.plot3_scaled = cam(gca);
  f = nf(); plot3([0 10], [0 100], [0 1000]); axis equal; out.plot3_scaled_equal = cam(gca);
  f = nf(); plot3([1 2 3], [4 5 6], [7 8 9]); view(25, 35); out.plot3_view25_35 = cam(gca);
  f = nf(); plot3([1 2 3], [4 5 6], [7 8 9]); view(25, 35);
  set(gca, 'xdir', 'reverse'); out.plot3_view25_35_xrev = cam(gca);
  f = nf('position', [0 0 1200 300]); surf([1 3 5; 2 4 6]); out.surf_fig1200x300 = cam(gca);
  f = nf(); subplot(2, 2, 1); surf([1 3 5; 2 4 6]); out.surf_subplot221 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); set(gca, 'plotboxaspectratio', [1 2 3]); out.surf_pbar123 = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); set(gca, 'projection', 'perspective'); out.surf_perspective = cam(gca);
  f = nf(); surf([1 3 5; 2 4 6]); set(gca, 'cameraviewangle', 20); out.surf_viewangle20 = cam(gca);
  % Fine sweep of view angles to separate the camera-distance rule from viewangle.
  out.view_sweep = {};
  for az = [-90 -37.5 0 30 60 90 135 180]
    for el = [-60 0 30 60 90]
      f = nf(); plot3([0 2], [0 3], [0 4]); view(az, el);
      out.view_sweep{end+1} = cam(gca);
    end
  end
end

% ---------------------------------------------------------------- g. colors
function out = m_colors()
  out = struct();
  out.defaultaxescolororder = mat(get(groot, 'defaultaxescolororder'));
  f = nf(); out.axes_colororder = mat(get(axes(), 'colororder'));
  cm = get(groot, 'defaultfigurecolormap');
  out.defaultfigurecolormap_size = vec(size(cm));
  out.defaultfigurecolormap_equals_viridis256 = isequal(cm, viridis(256));
  out.defaultfigurecolormap_maxabsdiff_viridis256 = max(abs(cm(:) - reshape(viridis(256), [], 1)));
  out.figure_colormap_size = vec(size(get(f, 'colormap')));
  out.viridis256 = mat(viridis(256));
  out.jet8 = mat(jet(8)); out.gray4 = mat(gray(4)); out.hot4 = mat(hot(4));
  out.viridis4 = mat(viridis(4)); out.jet64_size = vec(size(jet()));
  out.gray_noarg_size = vec(size(gray()));
  % colormap(gray) inside a figure: size it takes
  f = nf(); imagesc([0 1; 2 3]); colormap(gray); out.colormap_gray_in_figure_size = vec(size(colormap()));
  f = nf(); out.jet_noarg_in_figure_size = vec(size(jet()));
  if exist('parula'), out.parula4 = mat(parula(4)); else out.parula4 = 'parula does not exist in this Octave'; end
  if exist('turbo'), out.turbo4 = mat(turbo(4)); end
  % Line color cycling after 7 series
  f = nf(); h = plot(repmat(1:2, 9, 1)'); c = {};
  for i = 1:numel(h), c{i} = vec(get(h(i), 'color')); end
  out.plot9_colors = c;
  f = nf(); h1 = plot(1:3); hold on; h2 = plot(2:4); h3 = plot(3:5, 'r'); h4 = plot(4:6);
  out.hold_color_cycle = {vec(get(h1, 'color')), vec(get(h2, 'color')), vec(get(h3, 'color')), vec(get(h4, 'color'))};
end

% ---------------------------------------------------------------- h. defaults
function out = m_defaults()
  out = struct();
  f = nf(); h = plot(1:3); out.plot_line = lineinfo(h);
  f = nf(); h = plot(1:3, 'o'); out.plot_o = lineinfo(h);
  f = nf(); h = line([0 1], [0 1]); out.line_obj = lineinfo(h);
  f = nf(); h = line([0 1], [0 1], 'marker', 'o'); out.line_obj_marker_o = lineinfo(h);
  out.root_defaults = struct('defaultlinelinewidth', get(groot, 'defaultlinelinewidth'), ...
    'defaultlinemarkersize', get(groot, 'defaultlinemarkersize'), ...
    'defaultlinemarkerfacecolor', colorval(get(groot, 'defaultlinemarkerfacecolor')), ...
    'defaultlinemarkeredgecolor', colorval(get(groot, 'defaultlinemarkeredgecolor')), ...
    'defaultaxesfontsize', get(groot, 'defaultaxesfontsize'), ...
    'defaultaxesfontname', get(groot, 'defaultaxesfontname'), ...
    'defaulttextfontsize', get(groot, 'defaulttextfontsize'), ...
    'defaultaxeslinewidth', get(groot, 'defaultaxeslinewidth'), ...
    'defaultaxesticklength', {vec(get(groot, 'defaultaxesticklength'))}, ...
    'defaultaxesbox', get(groot, 'defaultaxesbox'), ...
    'defaultaxestitlefontsizemultiplier', get(groot, 'defaultaxestitlefontsizemultiplier'), ...
    'defaultaxeslabelfontsizemultiplier', get(groot, 'defaultaxeslabelfontsizemultiplier'), ...
    'defaultaxestitlefontweight', get(groot, 'defaultaxestitlefontweight'));
  f = nf(); h = scatter([1 2 3], [4 1 3]); k = allchild(h);
  out.scatter = struct('type', get(h, 'type'), 'sizedata', {vec(get(h, 'sizedata'))}, 'marker', get(h, 'marker'), ...
    'markerfacecolor', colorval(get(h, 'markerfacecolor')), 'markeredgecolor', colorval(get(h, 'markeredgecolor')), ...
    'cdata', {mat(get(h, 'cdata'))}, 'linewidth', get(h, 'linewidth'), 'children', {children_desc(h)});
  f = nf(); h = scatter([1 2 3], [4 1 3], [], 'filled');
  out.scatter_filled = struct('markerfacecolor', colorval(get(h, 'markerfacecolor')), 'markeredgecolor', colorval(get(h, 'markeredgecolor')), ...
    'sizedata', {vec(get(h, 'sizedata'))}, 'cdata', {mat(get(h, 'cdata'))});
  f = nf(); h = bar([1 2 3; 4 5 6]); b = {};
  for i = 1:numel(h)
    b{i} = struct('type', get(h(i), 'type'), 'barwidth', get(h(i), 'barwidth'), 'facecolor', colorval(get(h(i), 'facecolor')), ...
      'edgecolor', colorval(get(h(i), 'edgecolor')), 'basevalue', get(h(i), 'basevalue'), 'barlayout', get(h(i), 'barlayout'), ...
      'linewidth', get(h(i), 'linewidth'), 'children', {children_desc(h(i))});
    p = findall(h(i), 'type', 'patch');
    if ~isempty(p), b{i}.patch_facecolor = colorval(get(p(1), 'facecolor')); b{i}.patch_edgecolor = colorval(get(p(1), 'edgecolor'));
      b{i}.patch_cdata = mat(get(p(1), 'cdata')); b{i}.patch_cdatamapping = get(p(1), 'cdatamapping'); end
  end
  out.bar3series = b;
  out.bar3series_axes_children = children_desc(gca);
  f = nf(); h = area([1 3 2 4]); p = findall(h, 'type', 'patch');
  out.area = struct('facecolor', colorval(get(h, 'facecolor')), 'edgecolor', colorval(get(h, 'edgecolor')), ...
    'basevalue', get(h, 'basevalue'), 'patch_facecolor', colorval(get(p(1), 'facecolor')), 'linewidth', get(h, 'linewidth'));
  f = nf(); h = area([1 2; 3 4]); a = {};
  for i = 1:numel(h), a{i} = colorval(get(h(i), 'facecolor')); end
  out.area_2series_facecolor = a;
  f = nf(); h = stem([1 3 2 4]); l = findall(h, 'type', 'line'); ls = {};
  for i = 1:numel(l), ls{i} = lineinfo(l(i)); end
  bl = get(h, 'baseline');
  out.stem = struct('marker', get(h, 'marker'), 'color', colorval(get(h, 'color')), ...
    'markerfacecolor', colorval(get(h, 'markerfacecolor')), 'markersize', get(h, 'markersize'), ...
    'basevalue', get(h, 'basevalue'), 'lines', {ls}, 'baseline_type', get(bl, 'type'), 'baseline', lineinfo(bl), ...
    'baseline_visible', get(bl, 'visible'));
  f = nf(); plot(1:3); t = text(2, 2, 'merhaba');
  out.text = struct('fontsize', get(t, 'fontsize'), 'fontname', get(t, 'fontname'), 'margin', get(t, 'margin'), ...
    'horizontalalignment', get(t, 'horizontalalignment'), 'verticalalignment', get(t, 'verticalalignment'), ...
    'rotation', get(t, 'rotation'), 'color', colorval(get(t, 'color')), 'interpreter', get(t, 'interpreter'), ...
    'backgroundcolor', colorval(get(t, 'backgroundcolor')), 'edgecolor', colorval(get(t, 'edgecolor')), 'units', get(t, 'units'), ...
    'fontweight', get(t, 'fontweight'));
  ax = gca;
  out.axes = struct('fontsize', get(ax, 'fontsize'), 'fontname', get(ax, 'fontname'), 'linewidth', get(ax, 'linewidth'), ...
    'box', get(ax, 'box'), 'tickdir', get(ax, 'tickdir'), 'ticklength', {vec(get(ax, 'ticklength'))}, ...
    'xgrid', get(ax, 'xgrid'), 'gridcolor', colorval(get(ax, 'gridcolor')), 'gridalpha', get(ax, 'gridalpha'), ...
    'xcolor', colorval(get(ax, 'xcolor')), 'color', colorval(get(ax, 'color')), 'layer', get(ax, 'layer'), ...
    'title_fontsize', get(get(ax, 'title'), 'fontsize'), 'title_fontweight', get(get(ax, 'title'), 'fontweight'), ...
    'xlabel_fontsize', get(get(ax, 'xlabel'), 'fontsize'), 'nextplot', get(ax, 'nextplot'));
  out.figure = struct('color', colorval(get(f, 'color')), 'position', {vec(get(f, 'position'))}, 'units', get(f, 'units'));
  f = nf(); plot(1:3, 1:3, 1:3, 3:-1:1); lg = legend('a', 'b');
  out.legend = struct('location', get(lg, 'location'), 'orientation', get(lg, 'orientation'), 'box', get(lg, 'box'), ...
    'fontsize', get(lg, 'fontsize'), 'position', {vec(get(lg, 'position'))}, 'textcolor', colorval(get(lg, 'textcolor')), ...
    'ax_position_after_legend', {vec(get(gca, 'position'))});
  f = nf(); h = stairs([1 3 2 4]);
  out.stairs = struct('type', get(h, 'type'), 'color', colorval(get(h, 'color')), 'linewidth', get(h, 'linewidth'), 'children', {children_desc(h)});
  f = nf(); h = fill([0 1 0.5], [0 0 1], 'r');
  out.fill = struct('facecolor', colorval(get(h, 'facecolor')), 'edgecolor', colorval(get(h, 'edgecolor')), 'linewidth', get(h, 'linewidth'));
  f = nf(); h = patch([0 2 2 0], [0 0 1 1], [0 0.5 1]);
  out.patch = struct('facecolor', colorval(get(h, 'facecolor')), 'edgecolor', colorval(get(h, 'edgecolor')));
  f = nf(); h = surf([1 2; 3 4]);
  out.surf = struct('facecolor', colorval(get(h, 'facecolor')), 'edgecolor', colorval(get(h, 'edgecolor')), ...
    'linewidth', get(h, 'linewidth'), 'cdatamapping', get(h, 'cdatamapping'), 'box', get(gca, 'box'), ...
    'xgrid', get(gca, 'xgrid'), 'zgrid', get(gca, 'zgrid'));
  f = nf(); h = mesh([1 2; 3 4]);
  out.mesh = struct('facecolor', colorval(get(h, 'facecolor')), 'edgecolor', colorval(get(h, 'edgecolor')), 'xgrid', get(gca, 'xgrid'));
  f = nf(); h = imagesc([1 2; 3 4]);
  out.imagesc = struct('cdatamapping', get(h, 'cdatamapping'), 'xdata', {vec(get(h, 'xdata'))}, 'ydata', {vec(get(h, 'ydata'))}, ...
    'ydir', get(gca, 'ydir'), 'xlim', {vec(get(gca, 'xlim'))}, 'ylim', {vec(get(gca, 'ylim'))}, 'box', get(gca, 'box'), ...
    'layer', get(gca, 'layer'), 'ytick', {vec(get(gca, 'ytick'))}, 'xtick', {vec(get(gca, 'xtick'))});
end

% ---------------------------------------------------------------- i. geometry
function c = allpatches(ax)
  p = findall(ax, 'type', 'patch'); c = {};
  for i = numel(p):-1:1, c{end+1} = patchinfo(p(i)); end   % reversed: creation order
end
function out = m_geometry()
  out = struct();
  f = nf(); bar([3 5 2 4]); out.bar = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); bar([1 2; 3 4; 5 6]); out.bar_grouped = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); bar([1 2 3; 4 5 6]); out.bar_grouped3 = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); bar([1 2; 3 4], 'stacked'); out.bar_stacked = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); bar([1 -2; 3 4], 'stacked'); out.bar_stacked_neg = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); barh([2 4 1]); out.barh = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); bar([10 20 30], [1 2 3], 0.5); out.bar_x_width05 = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); bar([1 2 3], 'basevalue', 1.5); out.bar_basevalue = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); h = bar([3 5 2 4]); bl = get(h(1), 'baseline'); out.bar_baseline = lineinfo(bl);
  f = nf(); area([1 3 2 4]); out.area = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); area([1 2; 3 4; 2 1]); out.area_stacked = struct('patches', {allpatches(gca)}, 'axes', axinfo(gca));
  f = nf(); h = stairs([1 3 2 4]); l = findall(h, 'type', 'line'); ls = {};
  for i = 1:numel(l), ls{i} = lineinfo(l(i)); end
  out.stairs = struct('lines', {ls}, 'hg_xdata', {vec(get(h, 'xdata'))}, 'hg_ydata', {vec(get(h, 'ydata'))}, 'axes', axinfo(gca));
  f = nf(); h = stairs([1 2 4], [1 3 2]); l = findall(h, 'type', 'line'); ls = {};
  for i = 1:numel(l), ls{i} = lineinfo(l(i)); end
  out.stairs_xy = struct('lines', {ls});
  f = nf(); h = stem([1 3 2 4]); l = findall(gca, 'type', 'line'); ls = {};
  for i = 1:numel(l), ls{i} = lineinfo(l(i)); end
  out.stem = struct('lines_findall_axes', {ls}, 'axes', axinfo(gca));
  for nm = {'contour', 'contourf'}
    f = nf(); [c, h] = feval(nm{1}, [1 2 3; 2 3 4; 3 4 5]);
    out.(nm{1}) = struct('levellist', {vec(get(h, 'levellist'))}, 'levellistmode', get(h, 'levellistmode'), ...
      'levelstep', get(h, 'levelstep'), 'contourmatrix', {mat(c)}, 'fill', get(h, 'fill'), ...
      'linecolor', colorval(get(h, 'linecolor')), 'patches', {allpatches(h)}, 'children', {children_desc(h)}, ...
      'clim', {vec(get(gca, 'clim'))}, 'axes', axinfo(gca));
  end
  f = nf(); [c, h] = contour(peaks(8)); out.contour_peaks8 = struct('levellist', {vec(get(h, 'levellist'))}, ...
    'zrange', {vec([min(min(peaks(8))) max(max(peaks(8)))])}, 'levelstep', get(h, 'levelstep'));
  f = nf(); [c, h] = contour([0 1; 2 10]); out.contour_0_10 = struct('levellist', {vec(get(h, 'levellist'))}, 'levelstep', get(h, 'levelstep'));
  f = nf(); [c, h] = contour([0.1 0.2; 0.3 0.37]); out.contour_small = struct('levellist', {vec(get(h, 'levellist'))}, 'levelstep', get(h, 'levelstep'));
  f = nf(); [c, h] = contour([-5 0; 3 123]); out.contour_m5_123 = struct('levellist', {vec(get(h, 'levellist'))}, 'levelstep', get(h, 'levelstep'));
end

% ---------------------------------------------------------------- j. clim
function out = m_clim()
  out = struct();
  r = @(nm) struct('clim', {vec(get(gca, 'clim'))}, 'climmode', get(gca, 'climmode'), 'case', nm);
  out.cases = {};
  f = nf(); surf([1 2; 3 4]); out.cases{end+1} = r('surf_[1 2;3 4]');
  f = nf(); surf([5 5; 5 5]); out.cases{end+1} = r('surf_const5');
  f = nf(); surf([1 2; 3 4], [10 20; 30 40]); out.cases{end+1} = r('surf_with_C_10_40');
  f = nf(); imagesc([1 2; 3 4]); out.cases{end+1} = r('imagesc_[1 2;3 4]');
  f = nf(); imagesc([7 7; 7 7]); out.cases{end+1} = r('imagesc_const7');
  f = nf(); image([1 2; 3 4]); out.cases{end+1} = r('image_direct');
  f = nf(); bar([3 5 2 4]); out.cases{end+1} = r('bar_single');
  f = nf(); bar([1 2; 3 4; 5 6]); out.cases{end+1} = r('bar_grouped2');
  f = nf(); bar([1 2 3; 4 5 6]); out.cases{end+1} = r('bar_grouped3');
  f = nf(); bar([1 2; 3 4], 'stacked'); out.cases{end+1} = r('bar_stacked2');
  f = nf(); bar([1 2; 3 4; 5 6]); p = findall(gca, 'type', 'patch'); cd = {};
  for i = 1:numel(p), cd{i} = struct('cdata', {mat(get(p(i), 'cdata'))}, 'facevertexcdata', {mat(get(p(i), 'facevertexcdata'))}, 'cdatamapping', get(p(i), 'cdatamapping')); end
  out.bar_grouped2_patch_cdata = cd;
  f = nf(); plot(1:3); out.cases{end+1} = r('line');
  f = nf(); axes(); out.cases{end+1} = r('empty_axes');
  f = nf(); area([1 2; 3 4]); out.cases{end+1} = r('area2');
  f = nf(); patch([0 1 0], [0 0 1], [2; 5; 9]); out.cases{end+1} = r('patch_cdata_2_9');
  f = nf(); patch([0 1 0], [0 0 1], 'r'); out.cases{end+1} = r('patch_rgb');
  f = nf(); scatter(1:3, 1:3, 36, [4 8 9]); out.cases{end+1} = r('scatter_cdata_4_9');
  f = nf(); contour([1 2 3; 2 3 4; 3 4 5]); out.cases{end+1} = r('contour');
  f = nf(); contourf([1 2 3; 2 3 4; 3 4 5]); out.cases{end+1} = r('contourf');
  f = nf(); surf([1 2; 3 4]); hold on; surf([1 2; 3 4] + 10); out.cases{end+1} = r('surf_two_1_14');
  f = nf(); plot(1:3); hold on; surf([1 2; 3 4]); out.cases{end+1} = r('line_then_surf');
  % patches without colour data (bar/fill) mixed with scaled data
  f = nf(); bar([3 5]); hold on; surf([5 6; 7 8]); out.cases{end+1} = r('bar_then_surf_5_8');
  f = nf(); fill([0 1 0], [0 0 1], 'r'); hold on; imagesc([5 6; 7 8]); out.cases{end+1} = r('fill_rgb_then_imagesc_5_8');
  f = nf(); surf(zeros(2)); out.cases{end+1} = r('surf_const0');
  f = nf(); surf(-3 * ones(2)); out.cases{end+1} = r('surf_const_m3');
end
