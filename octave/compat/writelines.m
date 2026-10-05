function writelines(data, filename, varargin)
% WRITELINES Write a char vector, cellstr or string array with a line ending each.
% Supports .csv, .txt and .dat plus WriteMode overwrite/append.
  filename=local_filename(filename);local_extension(filename);mode=local_options(varargin{:});
  if isa(data,'string'),values=cellstr(data(:));
  elseif ischar(data)
    if ~isvector(data)&&~isequal(size(data),[0 0]),error('writelines: data must be a character vector, cellstr or string array');endif
    values={reshape(data,1,[])};
  elseif iscellstr(data),values=data(:);
  else,error('writelines: data must be char, cellstr or string');endif
  payload='';
  for k=1:numel(values),payload=[payload values{k} sprintf('\n')];endfor
  local_atomic_write(filename,payload,mode);
endfunction

function filename = local_filename(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('writelines: filename must be a character row vector or string scalar');endif
  filename=value;
endfunction

function local_extension(filename)
  [~,~,ext]=fileparts(filename);ext=lower(ext);
  if ~any(strcmp(ext,{'.csv','.txt','.dat'})),error('writelines: only .csv, .txt and .dat text files are supported');endif
endfunction

function mode = local_options(varargin)
  mode='overwrite';
  if mod(numel(varargin),2),error('writelines: options must be name/value pairs');endif
  for k=1:2:numel(varargin)
    name=varargin{k};if isa(name,'string')&&isscalar(name),name=char(name);endif
    if ~ischar(name)||~isrow(name),error('writelines: option names must be text');endif
    if ~strcmpi(name,'WriteMode'),error('writelines: unsupported option %s',name);endif
    value=varargin{k+1};if isa(value,'string')&&isscalar(value),value=char(value);endif
    if ~ischar(value)||~any(strcmpi(value,{'overwrite','append'})),error('writelines: WriteMode must be overwrite or append');endif
    mode=lower(value);
  endfor
endfunction

function local_atomic_write(filename,payload,mode)
  old='';
  if strcmp(mode,'append')&&exist(filename,'file')==2
    fid=fopen(filename,'rb');if fid<0,error('writelines: could not read existing file %s',filename);endif
    unwind_protect
      old=fread(fid,Inf,'*char')';
      [msg,err]=ferror(fid);if err,error('writelines: could not read existing file: %s',msg);endif
    unwind_protect_cleanup
      fclose(fid);
    end_unwind_protect
  endif
  [folder,~,~]=fileparts(filename);if isempty(folder),folder='.';endif
  % Exclusive same-directory creation; rename is the atomic commit point.
  % Unwind cleanup covers ordinary errors and catchable interruptions.
  % Forced process termination cannot run cleanup and may leave the temp file.
  temporary='';fid=-1;
  unwind_protect
    [fid,temporary,msg]=mkstemp(fullfile(folder,'.indymat-XXXXXX'));
    if fid<0,error('writelines: could not create temporary file: %s',msg);endif
    if fwrite(fid,[old payload],'char')~=numel(old)+numel(payload),error('writelines: incomplete write to temporary file');endif
    [msg,err]=ferror(fid);if err,error('writelines: write failed: %s',msg);endif
    status=fclose(fid);fid=-1;
    if status~=0,error('writelines: could not close temporary file');endif
    [err,msg]=rename(temporary,filename);if err~=0,error('writelines: could not replace destination: %s',msg);endif
  unwind_protect_cleanup
    if fid>=0,fclose(fid);endif
    if ~isempty(temporary)&&exist(temporary,'file')==2
      [err,msg]=unlink(temporary);
      if err~=0,error('writelines: could not remove temporary file %s: %s',temporary,msg);endif
    endif
  end_unwind_protect
endfunction
