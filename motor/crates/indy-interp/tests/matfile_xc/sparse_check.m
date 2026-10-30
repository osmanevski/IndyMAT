% Compares every variable of `files` with sparse_vals.m. Prints one row per
% variable for the first file, and the mismatches for all of them.
sparse_vals;
names = who('sp_*');
for f = 1:numel(files)
  got = load(files{f});
  bad = {};
  for k = 1:numel(names)
    n = names{k};
    if ~isfield(got, n), bad{end+1} = [n ':missing']; continue; end
    want = eval(n); have = got.(n);
    if ~sparse_same(want, have), bad{end+1} = n; end
    if issparse(want)
      [iw, jw, vw] = find(want); [ih, jh, vh] = find(have);
      if ~(isequal(iw(:), ih(:)) && isequal(jw(:), jh(:)) && isequaln(vw(:), vh(:)))
        bad{end+1} = [n ':triples'];
      end
      if f == 1
        fprintf('  %-12s issparse=%d class=%-7s size=%dx%d nnz=%d iscomplex=%d\n', n, issparse(have), class(have), size(have,1), size(have,2), nnz(have), ~isreal(have));
      end
    end
  end
  fprintf('%s: %d variables, mismatches: %s\n', files{f}, numel(names), strjoin(bad, ' '));
end
