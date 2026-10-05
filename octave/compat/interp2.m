function out = interp2 (varargin)
  if (nargin >= 2 && ischar (varargin{end-1}))
    out = original () (varargin{:}); return;
  endif
% INTERP2 Use NaN rather than Octave NA for the default missing fill value.
% Explicit extrapolation values pass through. All interpolation is Octave's.
  out = original () (varargin{:});
  out(isna (out)) = NaN;
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('interp2'); endif
  f = handle;
endfunction
