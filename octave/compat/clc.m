function clc()
% CLC Tell the IndyMAT command window to discard earlier output.
  if isappdata(0,'__mf_publishing__') && getappdata(0,'__mf_publishing__'),return;endif
  job=getappdata(0,'__mf_job__');
  if isempty(job),builtin('clc');return;endif
  [recording,~]=diary();
  if recording,diary off;endif
  unwind_protect
    fprintf('\n__MF_CLC_%s__\n',job);fflush(stdout);
  unwind_protect_cleanup
    if recording,diary on;endif
  end_unwind_protect
endfunction
