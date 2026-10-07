function varargout = __mf_sort_options__ (varargin)
% __MF_SORT_OPTIONS__ Numeric sort options; called only by the sort shadow.
% MissingPlacement and ComparisonMethod support real/complex arrays and stable
% indices. Other call forms return directly to the builtin. UTF-16 ordering
% and object-specific options remain their own class methods.
  args = varargin(2:end);
  first = find (cellfun (@(x) ischar (x) && any (strcmpi (x, ...
                {'MissingPlacement', 'ComparisonMethod'})), args), 1);
  if (isempty (first))
    [varargout{1:nargout}] = builtin ('sort', varargin{:});
    return;
  endif
  A = varargin{1};
  if (! (isnumeric (A) || islogical (A)))
    error ('sort: name-value options require a numeric or logical array');
  endif
  dim = find (size (A) != 1, 1);
  if (isempty (dim)), dim = 1; endif
  direction = 'ascend';
  positional = args(1:first-1);
  if (! isempty (positional) && isnumeric (positional{1}))
    dim = positional{1};
    positional(1) = [];
  endif
  if (! isempty (positional) && ischar (positional{1}))
    direction = lower (positional{1});
    positional(1) = [];
  endif
  if (! isempty (positional) || ! any (strcmp (direction, {'ascend','descend'})) ...
      || ! (isnumeric (dim) && isscalar (dim) && isreal (dim) ...
            && isfinite (dim) && dim >= 1 && dim == fix (dim)))
    error ('sort: invalid dimension or direction');
  endif
  missing = 'auto';
  method = 'auto';
  opts = args(first:end);
  if (mod (numel (opts), 2)), error ('sort: options require name-value pairs'); endif
  for k = 1:2:numel (opts)
    if (! ischar (opts{k}) || ! ischar (opts{k+1}))
      error ('sort: option names and values must be character vectors');
    endif
    if (strcmpi (opts{k}, 'MissingPlacement'))
      missing = lower (opts{k+1});
    elseif (strcmpi (opts{k}, 'ComparisonMethod'))
      method = lower (opts{k+1});
    else
      error ('sort: unknown option');
    endif
  endfor
  if (! any (strcmp (missing, {'auto','first','last'})) ...
      || ! any (strcmp (method, {'auto','real','abs'})))
    error ('sort: invalid option value');
  endif
  if (strcmp (missing, 'auto'))
    if (strcmp (direction, 'ascend')), missing = 'last'; else, missing = 'first'; endif
  endif
  if (dim > ndims (A))
    [varargout{1:nargout}] = builtin ('sort', A, dim, direction);
    return;
  endif
  order = [dim, 1:dim-1, dim+1:max(ndims (A), dim)];
  P = permute (A, order);
  shape = size (P);
  n = size (P, 1);
  P = reshape (P, n, prod (shape(2:end)));
  B = P;
  I = zeros (size (P));
  for column = 1:columns (P)
    x = P(:,column);
    absent = isnan (x);
    present = find (! absent);
    v = x(present);
    if (strcmp (method, 'auto') && isreal (A))
      [~, indices] = builtin ('sort', v, direction);
    elseif (strcmp (method, 'real'))
      keys = [real(v), imag(v)];
      if (strcmp (direction, 'descend')), cols = [-1 -2]; else, cols = [1 2]; endif
      [~, indices] = sortrows (keys, cols);
    else
      keys = [abs(v), angle(v)];
      if (strcmp (direction, 'descend')), cols = [-1 -2]; else, cols = [1 2]; endif
      [~, indices] = sortrows (keys, cols);
    endif
    present = present(indices);
    if (strcmp (missing, 'first')), indices = [find(absent); present];
    else, indices = [present; find(absent)]; endif
    B(:,column) = x(indices);
    I(:,column) = indices;
  endfor
  varargout{1} = ipermute (reshape (B, shape), order);
  if (nargout > 1), varargout{2} = ipermute (reshape (I, shape), order); endif
  if (nargout > 2), error ('sort: too many output arguments'); endif
endfunction
