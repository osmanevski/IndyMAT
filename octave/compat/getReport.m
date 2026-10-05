function report = getReport (err, varargin)
% GETREPORT Plain-text report for Octave's native caught error structs.
% Basic returns the message; extended appends name/file/line stack entries.
% Supports 'hyperlinks','off' only. No MException class/cause emulation.
  mode = 'extended';
  if (! isempty (varargin)), mode = varargin{1}; varargin(1) = []; endif
  if (! ischar (mode) || ! any (strcmp (mode, {'basic', 'extended'})))
    error ('getReport: format must be basic or extended');
  endif
  if (! isempty (varargin) && ! (numel (varargin) == 2 && strcmpi (varargin{1}, 'hyperlinks') && strcmpi (varargin{2}, 'off')))
    error ('getReport: only hyperlinks off is supported');
  endif
  if (! isstruct (err) || ! isscalar (err) || ! all (isfield (err, {'message', 'identifier', 'stack'})) || ! ischar (err.message) || ! ischar (err.identifier) || ! isstruct (err.stack))
    error ('getReport: expected a native caught error struct');
  endif
  report = err.message;
  if (strcmp (mode, 'extended'))
    if (! isempty (err.stack) && ! all (isfield (err.stack, {'name', 'file', 'line'})))
      error ('getReport: invalid error stack');
    endif
    for k = 1:numel (err.stack)
      frame = err.stack(k);
      if (! ischar (frame.name) || ! ischar (frame.file) || ! isnumeric (frame.line) || ! isscalar (frame.line))
        error ('getReport: invalid error stack entry');
      endif
      report = [report sprintf('\nError in %s (%s, line %d)', frame.name, frame.file, frame.line)];
    endfor
  endif
endfunction
