function varargout = fminsearch (varargin)
  if ((nargin != 2 && ! (nargin == 3 && isnumeric (varargin{3}) && isempty (varargin{3}))) || nargout > 4 || ! isa (varargin{1}, 'function_handle') || ! isa (varargin{2}, 'double') || ! isreal (varargin{2}) || issparse (varargin{2}) || isempty (varargin{2}) || any (! isfinite (varargin{2}(:))))
    [varargout{1:nargout}] = original () (varargin{:}); return;
  endif
% FMINSEARCH Default-options Nelder-Mead search for finite real double starts.
% Uses MATLAB's coordinate simplex (5 percent, or 0.00025 at zero) and
% absolute simplex/function tolerances. Keeps the input shape in objective
% calls and outputs. An empty numeric options argument also uses defaults.
% Nonempty options, problem structs, other classes and named
% objectives delegate to Octave; their simplex remains Octave's.
  fun = varargin{1}; x0 = varargin{2}; shape = size (x0); n = numel (x0);
  vertices = repmat (x0(:), 1, n+1);
  values = zeros (1, n+1);
  values(1) = evaluate (fun, vertices(:, 1), shape);
  for k = 1:n
    if (x0(k) == 0), vertices(k, k+1) = 0.00025;
    else, vertices(k, k+1) = 1.05 * x0(k); endif
    values(k+1) = evaluate (fun, vertices(:, k+1), shape);
  endfor
  count = n+1; iterations = 1; limit = 200*n; flag = 0;
  [values, order] = sort (values); vertices = vertices(:, order);
  while (count < limit && iterations < limit)
    spread_x = max (max (abs (vertices(:, 2:end) - vertices(:, 1))));
    spread_f = max (abs (values(2:end) - values(1)));
    if (spread_x <= max (1e-4, 10*eps (max (vertices(:, 1)))) && spread_f <= max (1e-4, 10*eps (values(1))))
      flag = 1; break;
    endif
    centroid = mean (vertices(:, 1:n), 2);
    reflected = 2*centroid - vertices(:, end);
    fr = evaluate (fun, reflected, shape); count += 1;
    shrink = false;
    if (fr < values(1))
      expanded = 3*centroid - 2*vertices(:, end);
      fe = evaluate (fun, expanded, shape); count += 1;
      if (fe < fr), vertices(:, end) = expanded; values(end) = fe;
      else, vertices(:, end) = reflected; values(end) = fr; endif
    elseif (fr < values(n))
      vertices(:, end) = reflected; values(end) = fr;
    elseif (fr < values(end))
      contracted = 1.5*centroid - 0.5*vertices(:, end);
      fc = evaluate (fun, contracted, shape); count += 1;
      if (fc <= fr), vertices(:, end) = contracted; values(end) = fc;
      else, shrink = true; endif
    else
      contracted = 0.5*centroid + 0.5*vertices(:, end);
      fc = evaluate (fun, contracted, shape); count += 1;
      if (fc < values(end)), vertices(:, end) = contracted; values(end) = fc;
      else, shrink = true; endif
    endif
    if (shrink)
      for k = 2:n+1
        vertices(:, k) = vertices(:, 1) + 0.5*(vertices(:, k) - vertices(:, 1));
        values(k) = evaluate (fun, vertices(:, k), shape);
      endfor
      count += n;
    endif
    [values, order] = sort (values); vertices = vertices(:, order);
    iterations += 1;
  endwhile
  if (flag == 1), message = 'fminsearch: optimization terminated successfully.';
  elseif (count >= limit), message = 'fminsearch: maximum number of function evaluations exceeded.';
  else, message = 'fminsearch: maximum number of iterations exceeded.'; endif
  % Default Display is notify: only unsuccessful termination is printed.
  if (flag == 0), fprintf ('%s\n', message); endif
  varargout = {reshape(vertices(:, 1), shape), values(1), flag, ...
    struct('iterations', iterations, 'funcCount', count, ...
           'algorithm', 'Nelder-Mead simplex direct search', 'message', message)};
endfunction

function value = evaluate (fun, x, shape)
  value = fun (reshape (x, shape));
  if (! isnumeric (value) || ! isscalar (value) || ! isreal (value))
    error ('fminsearch: objective must return a real numeric scalar');
  endif
endfunction

function f = original ()
  persistent handle;
  if (isempty (handle)), handle = __mf_original_function__ ('fminsearch'); endif
  f = handle;
endfunction
