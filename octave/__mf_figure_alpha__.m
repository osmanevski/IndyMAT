function [descriptor, code, args] = __mf_figure_alpha__(mode, raw, mapping, alim, alphamap, association, expected_count)
  % Pure alpha admission. Scalar face/edge alpha is documented by Octave.
  % Data-driven alpha semantics are not established by the property dump;
  % retain whole-figure fallback until a browser contract is approved.
  descriptor = []; code = ''; args = struct();
  if isnumeric(mode) && isreal(mode) && ~issparse(mode) && isscalar(mode) && isfinite(mode) && mode >= 0 && mode <= 1
    descriptor = struct('mode','constant','association','constant','value',double(mode));
    return;
  endif
  code = 'transparency'; args = struct('property','alpha','detail','unmeasured_data_mapping');
endfunction
