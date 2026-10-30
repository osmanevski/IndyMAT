% The level 4 file of sparse_vals.m: numeric variables only.
sparse_vals;
got = load(files{1});
names = fieldnames(got);
bad = {};
for k = 1:numel(names)
  n = names{k};
  if ~sparse_same(eval(n), got.(n)), bad{end+1} = n; end
end
fprintf('%s: %d variables, mismatches: %s\n', files{1}, numel(names), strjoin(bad, ' '));
