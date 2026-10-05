function value = input(prompt, mode)
% INPUT Forward an explicit input request to IndyMAT, preserving caller scope.
  if nargin<1,prompt='';endif
  if nargin>1 && ~strcmp(mode,'s'),error('input: second argument must be s');endif
  job=getappdata(0,'__mf_job__');
  folder=getappdata(0,'__mf_folder__');
  if isempty(folder),error('input: IndyMAT oturumu gerekli');endif
  response=fullfile(folder,'input.txt');
  while true
    if ~isempty(job),fprintf('\n__MF_INPUT_%s__\n',job);endif
    fprintf('%s',prompt);fflush(stdout);
    while ~exist(response,'file'),builtin('pause',0.025);endwhile
    line=fileread(response);unlink(response);
    if ~ischar(line),error('input: input stream closed');endif
    if nargin>1,value=line;return;endif
    if isempty(strtrim(line)),value=[];return;endif
    try,value=evalin('caller',line);return;
    catch err,fprintf(2,'%s\n',err.message);
    end_try_catch
  endwhile
endfunction
