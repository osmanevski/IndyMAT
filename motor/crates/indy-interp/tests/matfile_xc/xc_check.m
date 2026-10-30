% Load each MAT file in `files` and compare every variable of vals.m with
% isequal, class and size. Runs on the Rust engine and on octave-cli.
% Set `files` (cell of names).
vals; names = setdiff(who(), {'files', 'names'});
for k = 1:numel(files)
  T = load(files{k}); bad = {};
  for j = 1:numel(names)
    n = names{j};
    if ~isfield(T, n), bad{end+1} = [n '(missing)']; continue; end
    if ~isequal(T.(n), eval(n)) || ~strcmp(class(T.(n)), class(eval(n))) || ~isequal(size(T.(n)), size(eval(n)))
      bad{end+1} = n;
    end
  end
  fprintf('%s: %d variables, mismatches: %s\n', files{k}, numel(names), strjoin(bad, ' '));
end
