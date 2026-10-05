function varargout = rat (varargin)
  if (nargin < 1 || nargin > 2 || nargout > 1 || ! isfloat (varargin{1}) || ! isreal (varargin{1}))
    [varargout{1:nargout}] = original () (varargin{:}); return;
  endif
% RAT Real single-output continued-fraction text with parenthesized divisors.
% Octave supplies the approximation and tolerance handling. MATLAB encloses
% the final reciprocal divisor in parentheses even when it is positive.
% Numeric numerator/denominator outputs and complex inputs delegate unchanged.
  text = original () (varargin{:});
  if (isempty (text)), varargout{1} = text; return; endif
  lines = cell (rows (text), 1);
  for k = 1:rows (text)
    line = deblank (text(k, :));
    lines{k} = regexprep (line, '1/([0-9]+)(\)*)$', '1/($1)$2');
  endfor
  varargout{1} = char (lines);
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('rat'); endif
  f = handle;
endfunction
