function out = linspace (varargin)
  if (nargin < 2 || nargin > 3 || ! isa (varargin{1}, 'single') || ! isa (varargin{2}, 'single')), out = builtin ('linspace', varargin{:}); return; endif
% LINSPACE Single endpoints interpolate in double before one final single cast.
% Avoids Octave's premature single rounding of interior points. Scalar/vector
% and complex endpoint forms use the native dimension/count validation. Other
% endpoint classes delegate unchanged; no claim of bitwise MATLAB equivalence
% for all floating-point grids is made.
  args = varargin;
  args{1} = double (args{1});
  args{2} = double (args{2});
  out = single (builtin ('linspace', args{:}));
endfunction
