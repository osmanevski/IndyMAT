function out = __mf_character_pattern__ (name, characters, varargin)
% Build greedy ASCII character runs with positive exact/minimum/maximum counts.
% Zero-length runs, bounds above 65535 and options are outside this subset.
  if (numel (varargin) > 2), error ('%s: expected zero, one or two character counts', name); endif
  lo = 1; hi = Inf;
  if (! isempty (varargin)), lo = varargin{1}; hi = lo; endif
  if (numel (varargin) == 2), hi = varargin{2}; endif
  if (! isnumeric (lo) || ! isreal (lo) || ! isscalar (lo) || ! isfinite (lo) || lo < 1 || fix (lo) != lo || ...
      lo > 65535 || ! isnumeric (hi) || ! isreal (hi) || ! isscalar (hi) || isnan (hi) || hi < lo || ...
      (isfinite (hi) && (fix (hi) != hi || hi > 65535)))
    error ('%s: counts must be positive integers up to 65535 with MIN <= MAX; MAX may be Inf', name);
  endif
  if (isinf (hi)), quantifier = sprintf ('{%d,}', lo);
  else, quantifier = sprintf ('{%d,%d}', lo, hi); endif
  out = pattern.fromExpression ([characters quantifier], false);
endfunction
