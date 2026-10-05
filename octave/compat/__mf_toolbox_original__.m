function handle = __mf_toolbox_original__ (name)
% __MF_TOOLBOX_ORIGINAL__ Resolve an m-file past octave/compat once per caller.
% Octave 11.3's which('-all',name) returns only one result, so use its complete
% load-path file listing as fallback. Restore the path even if binding fails.
% Cache centrally: temporary precedence changes may clear shadow persistents.
  persistent handles = struct ();
  if (isfield (handles, name)), handle = handles.(name); return; endif
  candidates = file_in_loadpath ([name '.m'], 'all');
  compat = fileparts (mfilename ('fullpath'));
  for k = 1:numel (candidates)
    folder = fileparts (candidates{k});
    if (strcmp (canonicalize_file_name (folder), canonicalize_file_name (compat))), continue; endif
    saved = path ();
    unwind_protect
      addpath (folder, '-begin');
      handle = str2func (name);
      resolved = functions (handle);
      if (! strcmp (canonicalize_file_name (resolved.file), canonicalize_file_name (candidates{k})))
        error ('%s: could not bind original function', name);
      endif
    unwind_protect_cleanup
      path (saved);
    end_unwind_protect
    handles.(name) = handle;
    return;
  endfor
  error ('%s: original function is unavailable', name);
endfunction
