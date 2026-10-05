function out = bitget (varargin)
  if (nargin != 2 || ! isnumeric (varargin{1})), original = __mf_original_function__ ('bitget'); out = original (varargin{:}); return; endif
% BITGET Two-argument numeric form returns the input class, as MATLAB does.
% The original validates bit positions and performs extraction. Assumed-type
% forms and nonnumeric inputs retain Octave behaviour and diagnostics.
  original = __mf_original_function__ ('bitget');
  value = varargin{1};
  if (isinteger (value) && class (value)(1) == 'i')
    % Octave's signed workaround typecasts and flattens matrices. Preserve shape.
    unsigned = ['u' class(value)];
    bits = reshape (typecast (value(:), unsigned), size (value));
    out = original (bits, varargin{2});
  else
    out = original (varargin{:});
  endif
  out = cast (out, class (value));
endfunction
