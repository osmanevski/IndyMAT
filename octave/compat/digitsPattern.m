function out = digitsPattern (varargin)
% DIGITSPATTERN Greedy ASCII digit runs; positive exact/ranged counts or 1..Inf.
% Unicode digit classes, zero counts and name/value options are unsupported.
  out = __mf_character_pattern__ ('digitsPattern', '[0-9]', varargin{:});
endfunction
