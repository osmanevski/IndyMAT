function out = strtrim (varargin)
  if (nargin != 1 || ! isa (varargin{1}, 'string'))
    persistent original;
    if (isempty (original)), original = __mf_text_original__ ('strtrim'); endif
    out = original (varargin{:});
    return;
  endif
% STRTRIM Add the datatypes string form, keeping missing elements and shape.
% Char/cellstr forms delegate to Octave. String whitespace follows strip.
  out = strip (varargin{1});
endfunction
