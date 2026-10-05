function seconds_per_call = timeit (f, outputs)
% TIMEIT Measure a zero-input function handle, median of seven timed batches.
% One untimed warm-up; ~10 ms batches capped at 10,000 calls. Includes loop
% and output-assignment overhead, without MATLAB's calibration machinery.
% Default requests one output if available, otherwise zero. Anonymous and
% variable-output handles are probed using native Octave output diagnostics.
  if (nargin < 1 || nargin > 2 || ! isa (f, 'function_handle'))
    error ('timeit: expected a zero-input function handle and optional output count');
  endif
  if (! any (nargin (f) == [0 -1])), error ('timeit: function must accept zero inputs'); endif
  if (nargin == 1)
    outputs = nargout (f);
    if (outputs < 0)
      try
        value = f (); outputs = 1;
      catch err
        no_output = (strcmp (err.identifier, 'Octave:invalid-fun-call') && ! isempty (strfind (err.message, 'function called with too many outputs')));
        no_output |= isempty (err.identifier) && any (strcmp (err.message, {'value on right hand side of assignment is undefined', 'element number 1 undefined in return list'}));
        if (! no_output), rethrow (err); endif
        outputs = 0;
      end_try_catch
    endif
    outputs = min (outputs, 1);
  endif
  if (! isnumeric (outputs) || ! isreal (outputs) || ! isscalar (outputs) || ! isfinite (outputs) || outputs < 0 || fix (outputs) != outputs || outputs > 64)
    error ('timeit: output count must be an integer from 0 to 64');
  endif
  outputs = double (outputs); values = cell (1, outputs);
  % Warm-up also validates that the function really accepts zero inputs.
  if (outputs == 0), f (); else, [values{:}] = f (); endif
  timer = tic ();
  if (outputs == 0), f (); else, [values{:}] = f (); endif
  first = toc (timer);
  count = min (10000, max (1, ceil (0.01 / max (first, 1e-6))));
  samples = zeros (1, 7);
  for batch = 1:7
    timer = tic ();
    if (outputs == 0)
      for k = 1:count, f (); endfor
    else
      for k = 1:count, [values{:}] = f (); endfor
    endif
    samples(batch) = toc (timer) / count;
  endfor
  seconds_per_call = median (samples);
endfunction
