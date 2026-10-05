function out = alphanumericsPattern (varargin)
% ALPHANUMERICSPATTERN ASCII letter/digit runs; positive exact/ranged counts.
% Default is 1..Inf; Unicode classes, zero counts and options are unsupported.
  out = __mf_character_pattern__ ('alphanumericsPattern', '[A-Za-z0-9]', varargin{:});
endfunction
