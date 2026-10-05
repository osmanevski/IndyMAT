function code = __mf_exist_query__ (varargin)
% __MF_EXIST_QUERY__ Encode native exist arguments without caller variables.
% Character bytes, including quotes, newlines and NUL, become numeric literals;
% the expression cannot execute text supplied as a name. Function-handle literals
% keep it working when the caller has a variable named builtin. Char arrays retain
% their dimensions and native coercion/warnings; other types delegate directly.
  code = "(@builtin) ('exist'";
  for k = 1:builtin ('nargin')
    value = varargin{k};
    dimensions = sprintf ('%d,', builtin ('size', value));
    code = [code ", (@builtin) ('char', (@builtin) ('reshape', [" sprintf('%d,', builtin('double', value(:))) "], " dimensions(1:end-1) "))"];
  endfor
  code = [code ')'];
endfunction
