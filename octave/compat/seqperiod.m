function p = seqperiod (x)
% SEQPERIOD Return the shortest exact repetition length of a finite vector.
% Supports numeric/logical vectors; non-repeating finite data returns N.
  if ((! isnumeric (x) && ! islogical (x)) || ! isvector (x) || isempty (x))
    error ('seqperiod: x must be a nonempty numeric or logical vector');
  endif
  x = x(:); n = numel (x); p = n;
  for candidate = 1:n
    if (rem (n, candidate) == 0 && isequal (x, repmat (x(1:candidate), n/candidate, 1)))
      p = candidate; return;
    endif
  endfor
endfunction
