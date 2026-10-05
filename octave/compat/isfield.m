function out = isfield (varargin)
  if (nargin != 2 || ! iscell (varargin{2}) || ! isempty (varargin{2})), out = builtin ('isfield', varargin{:}); return; endif
% ISFIELD Empty cell field-name lists yield scalar false, as in MATLAB.
% Every nonempty field-name form and invalid arity delegates immediately.
  out = false;
endfunction
