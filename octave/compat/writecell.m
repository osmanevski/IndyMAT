function writecell(C, filename, varargin)
% WRITECELL Write scalar numeric, logical and text cells to delimited text.
% Supports .csv, .txt and .dat, Delimiter, and WriteMode overwrite/append.
% Single uses 7 significant digits, double 15; integers stay integers, logicals 0/1.
  if ~iscell(C)||ndims(C)>2,error('writecell: C must be a two-dimensional cell array');endif
  filename=local_filename(filename);local_extension(filename);
  [delimiter,mode]=local_options(varargin{:});
  pieces=cell(rows(C),1);
  for r=1:rows(C)
    fields=cell(1,columns(C));
    for c=1:columns(C),fields{c}=local_field(C{r,c},delimiter);endfor
    pieces{r}=[local_join(fields,delimiter) sprintf('\n')];
  endfor
  local_atomic_write(filename,[pieces{:}],mode);
endfunction

function filename = local_filename(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('writecell: filename must be a character row vector or string scalar');endif
  filename=value;
endfunction

function local_extension(filename)
  [~,~,ext]=fileparts(filename);ext=lower(ext);
  if ~any(strcmp(ext,{'.csv','.txt','.dat'})),error('writecell: only .csv, .txt and .dat text files are supported');endif
endfunction

function [delimiter,mode] = local_options(varargin)
  delimiter=',';mode='overwrite';
  if mod(numel(varargin),2),error('writecell: options must be name/value pairs');endif
  for k=1:2:numel(varargin)
    name=varargin{k};if isa(name,'string')&&isscalar(name),name=char(name);endif
    if ~ischar(name)||~isrow(name),error('writecell: option names must be text');endif
    switch lower(name)
      case 'delimiter',delimiter=local_delimiter(varargin{k+1});
      case 'writemode'
        value=varargin{k+1};if isa(value,'string')&&isscalar(value),value=char(value);endif
        if ~ischar(value)||~any(strcmpi(value,{'overwrite','append'})),error('writecell: WriteMode must be overwrite or append');endif
        mode=lower(value);
      otherwise,error('writecell: unsupported option %s',name);
    endswitch
  endfor
endfunction

function delimiter = local_delimiter(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('writecell: Delimiter must be text');endif
  switch lower(value)
    case 'comma',delimiter=',';case 'tab',delimiter=sprintf('\t');case 'space',delimiter=' ';
    case 'semicolon',delimiter=';';case 'bar',delimiter='|';
    otherwise,if numel(value)~=1,error('writecell: only a single-character Delimiter is supported');endif;delimiter=value;
  endswitch
endfunction

function field = local_field(value,delimiter)
  if isa(value,'missing')&&isscalar(value),field='';return;endif
  if isnumeric(value)&&isempty(value),field='';return;endif
  if isnumeric(value)&&isscalar(value)&&isreal(value)
    if isinteger(value)
      if strncmp(class(value),'uint',4),field=sprintf('%u',value);else,field=sprintf('%d',value);endif
    elseif isnan(value),field='NaN';elseif isinf(value)&&value>0,field='Inf';elseif isinf(value),field='-Inf';
    elseif isa(value,'single'),field=sprintf('%.7g',value);
    else,field=sprintf('%.15g',double(value));endif
    return;
  endif
  if islogical(value)&&isscalar(value),field=sprintf('%d',value);return;endif
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value),error('writecell: cells must contain scalar real numbers, scalar logicals, text, or missing values');endif
  field=value;
  if any(field==delimiter)||any(field=='"')||any(field==sprintf('\n'))||any(field==sprintf('\r'))
    field=['"' strrep(field,'"','""') '"'];
  endif
endfunction

function text = local_join(fields,delimiter)
  if isempty(fields),text='';return;endif
  text=fields{1};for k=2:numel(fields),text=[text delimiter fields{k}];endfor
endfunction

function local_atomic_write(filename,payload,mode)
  old='';
  if strcmp(mode,'append')&&exist(filename,'file')==2
    fid=fopen(filename,'rb');if fid<0,error('writecell: could not read existing file %s',filename);endif
    unwind_protect
      old=fread(fid,Inf,'*char')';
      [msg,err]=ferror(fid);if err,error('writecell: could not read existing file: %s',msg);endif
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
    if fid<0,error('writecell: could not create temporary file: %s',msg);endif
    if fwrite(fid,[old payload],'char')~=numel(old)+numel(payload),error('writecell: incomplete write to temporary file');endif
    [msg,err]=ferror(fid);if err,error('writecell: write failed: %s',msg);endif
    status=fclose(fid);fid=-1;
    if status~=0,error('writecell: could not close temporary file');endif
    [err,msg]=rename(temporary,filename);if err~=0,error('writecell: could not replace destination: %s',msg);endif
  unwind_protect_cleanup
    if fid>=0,fclose(fid);endif
    if ~isempty(temporary)&&exist(temporary,'file')==2
      [err,msg]=unlink(temporary);
      if err~=0,error('writecell: could not remove temporary file %s: %s',temporary,msg);endif
    endif
  end_unwind_protect
endfunction
