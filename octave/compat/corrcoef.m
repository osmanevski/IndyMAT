function varargout = corrcoef (varargin)
  if (! (nargin >= 3 && any (cellfun (@(v) ischar (v) && strcmpi (v, 'complete'), varargin(2:end)))))
    [varargout{1:nargout}] = original () (varargin{:}); return;
  endif
% CORRCOEF Preserve the number of variables when complete rows leave <2 samples.
% Validates through Octave first; ordinary/pairwise forms are untouched.
  [varargout{1:nargout}] = original () (varargin{:});
  x = varargin{1}; start = 2; rowvector = false;
  if (isnumeric (varargin{2}))
    x = [x(:) varargin{2}(:)]; start = 3;
  elseif (isvector (x)), rowvector = isrow (x); x = x(:); endif
  complete = false;
  for k = start:2:nargin-1
    if (strcmpi (varargin{k}, 'rows')), complete = strcmpi (varargin{k+1}, 'complete'); endif
  endfor
  if (complete && rowvector)
    [varargout{1:nargout}] = original () (x, varargin{2:end});
  endif
  if (complete && sum (! any (isnan (x), 2)) < 2)
    for k = 1:max (1, nargout), varargout{k} = NaN (columns (x)); endfor
  endif
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('corrcoef'); endif
  f = handle;
endfunction
