function out = null (varargin)
  if (! (nargin == 2 && ischar (varargin{2}) && strcmp (varargin{2}, 'r')))
    out = original () (varargin{:}); return;
  endif
% NULL Rational basis via reduced row echelon form for finite floating matrices.
% Uses floating elimination, as MATLAB's rational basis does, not exact fractions.
% Ordinary orthonormal/tolerance forms retain Octave behaviour.
  a = varargin{1};
  if (! isfloat (a) || ! ismatrix (a) || any (! isfinite (a(:))))
    error ('null: rational form requires a finite floating matrix');
  endif
  if (isempty (a)), out = eye (columns (a), class (a)); return; endif
  [reduced, pivots] = rref (a);
  n = columns (a); free = setdiff (1:n, pivots);
  out = zeros (n, numel (free), class (reduced));
  out(free, :) = eye (numel (free), class (reduced));
  out(pivots, :) = -reduced(1:numel (pivots), free);
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('null'); endif
  f = handle;
endfunction
