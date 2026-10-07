function varargout = sort (varargin)
  if (nargin <= 2 || (nargin == 3 && isnumeric (varargin{2})))
    [varargout{1:nargout}] = builtin ('sort', varargin{:});
    return;
  endif
% SORT Add numeric MissingPlacement and ComparisonMethod name-value forms.
% Native forms take one cheap guard and delegate to the builtin. The extended
% implementation is separate so its local workspace adds no native-path cost.
% Object overloads dispatch independently; no UTF-16 ordering is added.
  [varargout{1:nargout}] = __mf_sort_options__ (varargin{:});
endfunction
