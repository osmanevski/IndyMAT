function varargout = ifft (varargin)
  if (! (nargin >= 2 && ischar (varargin{end}) && any (strcmp (varargin{end}, {'symmetric', 'nonsymmetric'}))))
    [varargout{1:nargout}] = builtin ('ifft', varargin{:}); return;
  endif
% IFFT Accept the trailing symmetry flag for vector and dimension forms.
% Symmetric uses the nonnegative-frequency half, ignores the redundant half,
% and discards imaginary parts at DC/Nyquist. Other calls delegate immediately.
  flag = varargin{end}; varargin(end) = [];
  if (numel (varargin) > 3), error ('ifft: too many arguments'); endif
  if (strcmp (flag, 'nonsymmetric'))
    [varargout{1:nargout}] = builtin ('ifft', varargin{:}); return;
  endif
  x = varargin{1};
  if (! isfloat (x)), error ('ifft: symmetric input must be single or double'); endif
  if (numel (varargin) == 3), dim = varargin{3};
  else, dim = find (size (x) != 1, 1); if (isempty (dim)), dim = 1; endif; endif
  if (! isscalar (dim) || ! isnumeric (dim) || dim < 1 || fix (dim) != dim)
    error ('ifft: dimension must be a positive integer');
  endif
  n = size (x, dim);
  if (numel (varargin) >= 2 && ! isempty (varargin{2})), n = varargin{2}; endif
  if (! isscalar (n) || ! isnumeric (n) || ! isreal (n) || ! isfinite (n) || n < 0 || fix (n) != n)
    error ('ifft: transform length must be a nonnegative integer');
  endif
  % Let the builtin validate/pad/truncate even the zero-length form.
  if (n == 0 || isempty (x)), varargout{1} = real (builtin ('ifft', varargin{:})); return; endif
  shape = size (x); shape(end+1:dim) = 1; shape(dim) = n;
  spectrum = zeros (shape, class (x));
  idx = repmat ({':'}, 1, max (ndims (x), dim));
  retained = min (size (x, dim), floor (n/2)+1);
  idx{dim} = 1:retained; spectrum(idx{:}) = x(idx{:});
  idx{dim} = 1; spectrum(idx{:}) = real (spectrum(idx{:}));
  if (mod (n, 2) == 0)
    idx{dim} = n/2+1; spectrum(idx{:}) = real (spectrum(idx{:}));
  endif
  source = idx; source{dim} = 2:ceil (n/2);
  idx{dim} = n:-1:floor (n/2)+2;
  spectrum(idx{:}) = conj (spectrum(source{:}));
  varargout{1} = real (builtin ('ifft', spectrum, [], dim));
endfunction
