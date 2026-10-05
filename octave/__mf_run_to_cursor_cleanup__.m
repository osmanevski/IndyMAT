function __mf_run_to_cursor_cleanup__(folder,file,line,restore,condition)
  mlock();actual=line;old_interrupt=false;
  try
    values=str2double(strsplit(strtrim(fileread(fullfile(folder,'run-to-cursor.txt'))),'\n'));
    if !isempty(values) && isfinite(values(1)),actual=values(1);endif
    if numel(values)>1 && isfinite(values(2)),old_interrupt=logical(values(2));endif
  catch
  end_try_catch
  [~,name]=fileparts(file);
  % Match setup: remove/restore the breakpoint on the still executing body.
  old_time_stamp=ignore_function_time_stamp('all');
  unwind_protect
    % dbclear on a command-line function crashes Octave 11.3; skip those names.
    if exist(name)~=103,try,dbclear(name,num2str(actual));catch,end_try_catch,endif
    if restore
      if isempty(condition),dbstop(name,num2str(line));
      else dbstop(name,num2str(line),'if',condition);endif
    endif
  unwind_protect_cleanup
    ignore_function_time_stamp(old_time_stamp);
    debug_on_interrupt(old_interrupt);
  end_unwind_protect
endfunction
