function tf = isStringScalar (value)
% ISSTRINGSCALAR True for a 1-by-1 datatypes string, including empty/missing.
% Other types return false. This subset has no UTF-16 or storage dependency.
  if (nargin != 1), error ('isStringScalar: one input is required'); endif
  tf = isa (value, 'string') && isscalar (value);
endfunction
