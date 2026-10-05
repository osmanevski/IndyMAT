function out = interp1 (varargin)
  if (! any (cellfun (@(v) ischar (v) && strcmp (v, 'makima'), varargin)))
    out = original () (varargin{:}); return;
  endif
% INTERP1 Integrate the finite real double MAKIMA subset with query/pp forms.
% Supports implicit/explicit sites, array values, scalar fill and extrapolation.
% Other methods delegate. Single, missing and complex samples are unsupported.
  method = find (cellfun (@(v) ischar (v) && strcmp (v, 'makima'), varargin));
  if (numel (method) != 1), error ('interp1: invalid method arguments'); endif
  ppform = (ischar (varargin{end}) && strcmp (varargin{end}, 'pp'));
  if (ppform)
    if (method == 3 && nargin == 4), x = varargin{1}; y = varargin{2};
    elseif (method == 2 && nargin == 3), y = varargin{1}; x = sites (y);
    else, error ('interp1: invalid makima pp form'); endif
  else
    if (method == 4), x = varargin{1}; y = varargin{2}; query = varargin{3};
    elseif (method == 3), y = varargin{1}; x = sites (y); query = varargin{2};
    else, error ('interp1: invalid makima query form'); endif
    if (nargin > method+1), error ('interp1: too many arguments'); endif
    if (! isa (query, 'double') || ! isreal (query)), error ('interp1: makima query points must be real double values'); endif
  endif
  if (! isa (x, 'double') || ! isreal (x) || ! isvector (x) || numel (x) < 2 || any (! isfinite (x(:))))
    error ('interp1: makima sites must be a finite real double vector with at least two points');
  endif
  if (! isa (y, 'double')), error ('interp1: makima sample values must be double'); endif
  x = double (x(:));
  if (isvector (y))
    if (numel (y) != numel (x)), error ('interp1: sites and values must have matching lengths'); endif
    y = y(:); dimensions = [];
  else
    dimensions = size (y); dimensions = dimensions(2:end);
    if (size (y, 1) != numel (x)), error ('interp1: first value dimension must match sites'); endif
  endif
  [x, order] = sort (x);
  data = reshape (y, numel (x), []); data = data(order, :).';
  if (isempty (dimensions)), values = data;
  else, values = reshape (data, [dimensions numel(x)]); endif
  pp = makima (x, values);
  if (ppform), out = pp; return; endif
  % interp1 places query dimensions first, ppval places value dimensions first.
  flat = ppval (pp, query(:).');
  flat = reshape (flat, [], numel (query)).';
  if (nargin == method+1)
    extrap = varargin{end};
    if (ischar (extrap) && strcmp (extrap, 'extrap')), fill = false;
    elseif (isnumeric (extrap) && isscalar (extrap)), fill = true;
    else, error ('interp1: extrapolation must be extrap or a numeric scalar'); endif
  else, fill = false; endif
  % MAKIMA, like spline/pchip, extrapolates by default.
  if (fill), flat(query(:) < x(1) | query(:) > x(end), :) = extrap; endif
  if (isempty (dimensions)), out = reshape (flat, size (query));
  else, out = reshape (flat, [size(query) dimensions]);
    if (isvector (query)), out = reshape (flat, [numel(query) dimensions]); endif
  endif
endfunction

function x = sites (y)
  if (isvector (y)), x = 1:numel (y); else, x = 1:size (y, 1); endif
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('interp1'); endif
  f = handle;
endfunction
