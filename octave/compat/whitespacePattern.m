function out = whitespacePattern (varargin)
% WHITESPACEPATTERN ASCII whitespace runs; positive exact/ranged counts.
% Default is 1..Inf; Unicode classes, zero counts and options are unsupported.
  out = __mf_character_pattern__ ('whitespacePattern', '[\x09-\x0D\x20]', varargin{:});
endfunction
