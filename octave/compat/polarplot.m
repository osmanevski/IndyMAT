function h = polarplot(theta, rho, varargin)
% POLARPLOT Polar lines using Octave polar; supports a linespec and line properties.
  if nargin<2, error('polarplot: theta and rho are required'); endif
  if ~isnumeric(theta) || ~isnumeric(rho) || ~isequal(size(theta),size(rho))
    error('polarplot: theta and rho must be same-sized numeric arrays');
  endif
  [spec,props]=__mf_polarplot_options__(varargin);
  if isempty(spec), h=polar(theta,rho); else h=polar(theta,rho,spec); endif
  if ~isempty(props), set(h,props{:}); endif
endfunction
