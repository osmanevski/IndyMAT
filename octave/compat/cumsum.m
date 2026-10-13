function varargout = cumsum (varargin)
% CUMSUM Reject Octave's all-dimensions extension; delegate other forms.
  for k = 2:nargin
    if (ischar (varargin{k}) && strcmpi (varargin{k}, 'all'))
      error ('MATLAB:cumsum:wrongFlag', 'The all option is not supported by cumsum.');
    endif
  endfor
  [varargout{1:nargout}] = builtin ('cumsum', varargin{:});
endfunction
