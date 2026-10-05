function __mf_run_to_cursor__(folder,file,line)
  mlock();
  target=fullfile(folder,'run-to-cursor.txt');error_file=fullfile(folder,'run-to-cursor-error.txt');
  try
    old_interrupt=debug_on_interrupt();fid=fopen(target,'w');if fid<0,error(__mf_text__('Could not save the temporary breakpoint.', 'Geçici kesme noktası kaydedilemedi.'));endif
    fprintf(fid,'%d\n%d',line,old_interrupt);fclose(fid);
    % Resolve the breakpoint against the loaded body, even for a file created
    % after engine startup. Timestamp checking can otherwise reparse that file
    % and attach dbstop to an inactive copy. Restore the user's setting always.
    old_time_stamp=ignore_function_time_stamp('all');
    unwind_protect
      [parent,name]=fileparts(file);addpath(parent,'-begin');
      if exist(name)==103,error(__mf_text__('"%s" is a command-line function; a breakpoint cannot be set.', '"%s" bir komut satırı işlevi; kesme noktası kurulamaz.'),name);endif
      try,dbclear(name,num2str(line));catch,end_try_catch
      actual=dbstop(name,num2str(line));
    unwind_protect_cleanup
      ignore_function_time_stamp(old_time_stamp);
    end_unwind_protect
    fid=fopen(target,'w');if fid<0,error(__mf_text__('Could not save the temporary breakpoint.', 'Geçici kesme noktası kaydedilemedi.'));endif
    fprintf(fid,'%d\n%d',actual,old_interrupt);fclose(fid);debug_on_interrupt(true);
  catch err
    fid=fopen(error_file,'w');if fid>=0,fputs(fid,err.message);fclose(fid);endif
    fprintf(2,__mf_text__('Could not run to cursor: %s\n', 'İmlece kadar çalıştırılamadı: %s\n'),err.message);
  end_try_catch
  [~,job]=fileparts(folder);fprintf('\n__MF_RTC_READY_%s__\n',job);fflush(stdout);
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
