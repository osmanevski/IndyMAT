function __mf_recover__(folder, ticket, cleanup, trace_path)
  % Recovery never evaluates user code. Its current-ticket result is reused if
  % the original DONE is in transit or readline delivers a duplicate attempt.
  mlock();
  if !strcmp(ticket,getenv('INDYMAT_DEBUG_TICKET')), return; endif
  job=regexp(folder,'[^/\\]+$','match','once');
  completed=[folder filesep() 'recovery-' ticket '.done'];
  if exist(completed,'file')~=2
    if !isempty(cleanup), eval(cleanup); endif
    if exist([folder filesep() 'result.json'],'file')~=2
      (@__mf_execute__)(folder,'snapshot','');
    endif
    fid=fopen(completed,'w');
    if fid>=0, fclose(fid); endif
  endif
  fid=fopen(trace_path,'a');
  if fid>=0
    fprintf(fid,'R %s\n',ticket); fflush(fid); fclose(fid);
    (@__mf_marker__)(['__MF_RECOVER_' ticket '__']);
  endif
endfunction
