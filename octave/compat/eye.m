function out = eye (varargin)
  if (nargin < 2 || ! isequal (varargin{end-1}, 'like')), out = builtin ('eye', varargin{:}); return; endif
% EYE Trailing 'like',prototype for numeric/logical arrays, including sparse
% and complex storage. Dimensions and ordinary class forms delegate to Octave.
% Object prototypes are unsupported; class overload dispatch remains native.
  prototype = varargin{end};
  if (! isnumeric (prototype) && ! islogical (prototype))
    error ('eye: like prototype must be numeric or logical');
  endif
  if (issparse (prototype))
    out = cast (speye (varargin{1:end-2}), class (prototype));
  else
    out = builtin ('eye', varargin{1:end-2}, class (prototype));
  endif
  if (! isreal (prototype)), out = complex (out); endif
endfunction
