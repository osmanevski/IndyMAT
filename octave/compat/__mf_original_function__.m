function original = __mf_original_function__ (name)
% __MF_ORIGINAL_FUNCTION__ Cache an original m-file behind this compat directory.
% Octave 11 has no which -all implementation; file_in_path supplies its fallback.
% Resolution briefly changes directory, restores it even on failure, and never
% changes the load path. Handles are bound once, before the directory is restored.
  persistent handles = struct ();
  if (isfield (handles, name)), original = handles.(name); return; endif
  compat = fileparts (mfilename ('fullpath'));
  warning_state = warning ('query', 'Octave:language-extension');
  unwind_protect
    warning ('off', 'Octave:language-extension');
    candidates = which (name, '-all');
  unwind_protect_cleanup
    warning (warning_state.state, 'Octave:language-extension');
  end_unwind_protect
  if (ischar (candidates)), candidates = {candidates}; endif
  candidates = [candidates(:); file_in_path(path (), [name '.m'], 'all')(:)];
  target = '';
  for k = 1:numel (candidates)
    candidate = candidates{k};
    if (! strcmp (fileparts (candidate), compat) && exist (candidate, 'file') == 2)
      target = candidate;
      break;
    endif
  endfor
  if (isempty (target)), error ('%s: original function could not be resolved', name); endif
  previous = pwd ();
  unwind_protect
    cd (fileparts (target));
    original = str2func (name);
    info = functions (original);
    if (! strcmp (info.file, target)), error ('%s: original function resolution failed', name); endif
  unwind_protect_cleanup
    cd (previous);
  end_unwind_protect
  handles.(name) = original;
endfunction
