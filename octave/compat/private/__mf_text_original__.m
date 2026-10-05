function fn = __mf_text_original__ (name)
% Resolve and cache an original .m function beyond octave/compat.
% Octave 11.3's which -all returns only one result; file_in_loadpath provides
% the complete ordered list on that version. Handles keep the resolved file.
  persistent handles = struct ();
  if (! isfield (handles, name))
    [last_message, last_identifier] = lastwarn ();
    evalc ("candidates = which (name, '-all');");
    lastwarn (last_message, last_identifier);
    if (ischar (candidates)), candidates = {candidates}; endif
    files = file_in_loadpath ([name '.m'], 'all');
    candidates = [candidates(:); files(:)];
    compat = fileparts (fileparts (mfilename ('fullpath')));
    selected = '';
    for k = 1:numel (candidates)
      candidate = candidates{k};
      if (exist (candidate, 'file') == 2 && ! strcmp (fileparts (candidate), compat))
        selected = candidate;
        break;
      endif
    endfor
    if (isempty (selected)), error ('%s: original function was not found', name); endif
    previous = pwd ();
    previous_path = path ();
    unwind_protect
      cd (fileparts (selected));
      handles.(name) = str2func (name);
    unwind_protect_cleanup
      cd (previous);
      if (! strcmp (path (), previous_path)), path (previous_path); endif
    end_unwind_protect
  endif
  fn = handles.(name);
endfunction
