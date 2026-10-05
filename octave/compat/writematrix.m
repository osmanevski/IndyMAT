function writematrix(A, filename, varargin)
% WRITEMATRIX Write a real numeric matrix to a delimited text file.
% Supports .csv, .txt and .dat, Delimiter, and WriteMode overwrite/append.
% Floating-point values use 7 significant digits for single, 15 for double.
% Sparse input is unsupported.
  if ~(isnumeric(A)||islogical(A))||ndims(A)>2||~isreal(A)||issparse(A),error('writematrix: A must be a full real numeric or logical matrix (sparse is unsupported)');endif
  filename=local_filename(filename);local_extension(filename);
  [delimiter,mode]=local_options(varargin{:});
  pieces=cell(rows(A),1);
  for r=1:rows(A)
    fields=cell(1,columns(A));
    for c=1:columns(A),fields{c}=local_number(A(r,c));endfor
    pieces{r}=[local_join(fields,delimiter) sprintf('\n')];
  endfor
  local_atomic_write(filename,[pieces{:}],mode);
endfunction

function filename = local_filename(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('writematrix: filename must be a character row vector or string scalar');endif
  filename=value;
endfunction

function local_extension(filename)
  [~,~,ext]=fileparts(filename);ext=lower(ext);
  if ~any(strcmp(ext,{'.csv','.txt','.dat'})),error('writematrix: only .csv, .txt and .dat text files are supported');endif
endfunction

function [delimiter,mode] = local_options(varargin)
  delimiter=',';mode='overwrite';
  if mod(numel(varargin),2),error('writematrix: options must be name/value pairs');endif
  for k=1:2:numel(varargin)
    name=varargin{k};if isa(name,'string')&&isscalar(name),name=char(name);endif
    if ~ischar(name)||~isrow(name),error('writematrix: option names must be text');endif
    switch lower(name)
      case 'delimiter',delimiter=local_delimiter(varargin{k+1});
      case 'writemode'
        value=varargin{k+1};if isa(value,'string')&&isscalar(value),value=char(value);endif
        if ~ischar(value)||~any(strcmpi(value,{'overwrite','append'})),error('writematrix: WriteMode must be overwrite or append');endif
        mode=lower(value);
      otherwise,error('writematrix: unsupported option %s',name);
    endswitch
  endfor
endfunction

function delimiter = local_delimiter(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('writematrix: Delimiter must be text');endif
  switch lower(value)
    case 'comma',delimiter=',';case 'tab',delimiter=sprintf('\t');case 'space',delimiter=' ';
    case 'semicolon',delimiter=';';case 'bar',delimiter='|';
    otherwise,if numel(value)~=1,error('writematrix: only a single-character Delimiter is supported');endif;delimiter=value;
  endswitch
endfunction

function text = local_number(value)
  if isinteger(value)
    if strncmp(class(value),'uint',4),text=sprintf('%u',value);else,text=sprintf('%d',value);endif
  elseif isnan(value),text='NaN';
  elseif isinf(value)&&value>0,text='Inf';
  elseif isinf(value),text='-Inf';
  elseif isa(value,'single'),text=sprintf('%.7g',value);
  else,text=sprintf('%.15g',double(value));endif
endfunction

function text = local_join(fields,delimiter)
  if isempty(fields),text='';return;endif
  text=fields{1};for k=2:numel(fields),text=[text delimiter fields{k}];endfor
endfunction

function local_atomic_write(filename,payload,mode)
  old='';
  if strcmp(mode,'append')&&exist(filename,'file')==2
    fid=fopen(filename,'rb');if fid<0,error('writematrix: could not read existing file %s',filename);endif
    unwind_protect
      old=fread(fid,Inf,'*char')';
      [msg,err]=ferror(fid);if err,error('writematrix: could not read existing file: %s',msg);endif
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
    if fid<0,error('writematrix: could not create temporary file: %s',msg);endif
    if fwrite(fid,[old payload],'char')~=numel(old)+numel(payload),error('writematrix: incomplete write to temporary file');endif
    [msg,err]=ferror(fid);if err,error('writematrix: write failed: %s',msg);endif
    status=fclose(fid);fid=-1;
    if status~=0,error('writematrix: could not close temporary file');endif
    [err,msg]=rename(temporary,filename);if err~=0,error('writematrix: could not replace destination: %s',msg);endif
  unwind_protect_cleanup
    if fid>=0,fclose(fid);endif
    if ~isempty(temporary)&&exist(temporary,'file')==2
      [err,msg]=unlink(temporary);
      if err~=0,error('writematrix: could not remove temporary file %s: %s',temporary,msg);endif
    endif
  end_unwind_protect
endfunction
