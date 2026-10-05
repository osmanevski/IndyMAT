function varargout = periodogram (x, varargin)
  if (nargin < 3 || ! isnumeric (varargin{2}) || ! isvector (varargin{2}) || numel (varargin{2}) < 2)
    persistent original;
    if (isempty (original)), original = __mf_original_function__ ('periodogram'); endif
    [varargout{1:nargout}] = original (x, varargin{:}); return;
  endif
% PERIODOGRAM Add the explicit-frequency-vector form for double/single vectors.
% Frequencies preserve their input shape; this form estimates a two-sided PSD.
% Scalar FFT lengths and all other existing forms delegate to Octave.
% Matrix inputs, confidence intervals and range options are unsupported here.
  if (nargin > 4), error ('periodogram: frequency-vector form accepts at most four inputs'); endif
  window = varargin{1}; f = varargin{2}; fs = 2*pi;
  if (nargin == 4 && ! isempty (varargin{3})), fs = varargin{3}; endif
  p = __mf_frequency_psd__ (x, window, f, fs);
  if (nargout == 0)
    plot (f, 10*log10(p)); xlabel ('Frequency'); ylabel ('Power density');
  else
    varargout{1} = p;
    if (nargout > 1), varargout{2} = f; endif
  endif
endfunction
