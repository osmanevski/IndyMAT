function varargout = prctile (varargin)
  if (nargin == 0 || (! isinteger (varargin{1}) && ! (nargin >= 4 && ischar (varargin{end-1}) && strcmpi (varargin{end-1}, 'Method')))), [varargout{1:nargout}] = original (varargin{:}); return; endif
% PRCTILE Integer output type and named midpoint/inclusive/exclusive methods.
% Percentiles retain Octave's dimension/NaN semantics; numeric method forms
% remain Octave extensions. Named methods map to Hyndman-Fan 5, 7 and 6.
  args = __mf_quantile_options__ (varargin);
  type = class (args{1}); integer = isinteger (args{1});
  if (integer), args{1} = double (args{1}); endif
  [varargout{1:nargout}] = original (args{:});
  if (integer && nargout > 0), varargout{1} = cast (varargout{1}, type); endif
endfunction

function varargout = original (varargin)
  persistent handle;
  if (isempty (handle)), handle = __mf_toolbox_original__ ('prctile'); endif
  [varargout{1:nargout}] = handle (varargin{:});
endfunction
