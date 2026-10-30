function ok = sparse_same(a, b)
  % Same class, size, storage and values, also inside cells and structs.
  % Octave 11 reads MATLAB's logical sparse layout (mxSPARSE with the logical
  % flag) as a double sparse matrix: storage and values survive, not the class.
  if exist('OCTAVE_VERSION', 'builtin') && issparse(a) && islogical(a) && ~islogical(b)
    a = double(a);
  end
  ok = strcmp(class(a), class(b)) && isequal(size(a), size(b));
  if ~ok, return; end
  if iscell(a)
    for k = 1:numel(a), ok = ok && sparse_same(a{k}, b{k}); end
  elseif isstruct(a)
    f = fieldnames(a); ok = isequal(f, fieldnames(b));
    for e = 1:numel(a), for k = 1:numel(f), ok = ok && sparse_same(a(e).(f{k}), b(e).(f{k})); end, end
  elseif isnumeric(a) || islogical(a)
    ok = issparse(a) == issparse(b) && isreal(a) == isreal(b) && nnz(a) == nnz(b);
    if issparse(a)
      [i, j, v] = find(a); [p, q, w] = find(b);
      ok = ok && isequal(i(:), p(:)) && isequal(j(:), q(:)) && isequaln(v(:), w(:));
    else
      ok = ok && isequaln(a, b);
    end
  else
    ok = isequal(a, b);
  end
end
