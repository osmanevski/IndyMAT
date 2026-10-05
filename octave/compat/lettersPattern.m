function out = lettersPattern (varargin)
% LETTERSPATTERN Greedy ASCII letter runs; positive exact/ranged counts or 1..Inf.
% Unicode letter classes, zero counts and name/value options are unsupported.
  out = __mf_character_pattern__ ('lettersPattern', '[A-Za-z]', varargin{:});
endfunction
