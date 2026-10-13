## MATLAB R2025b inc2: batch population of empty models; partial sizes reject.
function sys = __validate_sizes__ (sys)
  dims = size(sys.data.K);
  if (!isequal(size(sys.data.Z),dims) || !isequal(size(sys.data.P),dims))
    error('Control:ltiobject:zpkProperties4', ...
          'The values of the "Z", "P" and "K" properties must have compatible sizes.');
  endif
  % Channel metadata lives in data; parent name lengths encode LTI size.
  % Empty models can store pending names without acquiring channels (inc3).
  names = {sys.data.InputName, sys.data.OutputName};
  fields = {'inname','outname'};
  counts = [dims(2),dims(1)];
  for j=1:2
    value = names{j};
    if (numel(value) != counts(j)), value=repmat({''},counts(j),1); endif
    sys.lti=builtin('subsasgn',sys.lti,substruct('.',fields{j}),value);
  endfor
endfunction
