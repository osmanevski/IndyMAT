function lines = readlines(filename, varargin)
% READLINES Read .csv, .txt or .dat text as a column string array.
% Recognizes LF, CRLF and CR. EmptyLineRule: read (default) or skip.
% A terminal line ending leaves a final empty string; an empty file yields "".
% Consumes a leading UTF-8 BOM.
  rule=local_options(varargin{:});
  filename=local_filename(filename);local_extension(filename);
  fid=fopen(filename,'rb');if fid<0,error('readlines: could not open file %s',filename);endif
  unwind_protect,text=fread(fid,Inf,'*char')';unwind_protect_cleanup,fclose(fid);end_unwind_protect
  if numel(text)>=3&&isequal(uint8(text(1:3)),uint8([239 187 191])),text=text(4:end);endif
  text=strrep(text,sprintf('\r\n'),sprintf('\n'));text=strrep(text,sprintf('\r'),sprintf('\n'));
  values=cell(0,1);start=1;
  for stop=find(text==sprintf('\n'))
    values{end+1,1}=text(start:stop-1);start=stop+1;
  endfor
  values{end+1,1}=text(start:end);
  if strcmp(rule,'skip'),values=values(~cellfun(@isempty,values));endif
  values=reshape(values,[],1);
  lines=string(values);
endfunction

function rule = local_options(varargin)
  rule='read';
  if mod(numel(varargin),2),error('readlines: options must be name/value pairs');endif
  for k=1:2:numel(varargin)
    name=varargin{k};if isa(name,'string')&&isscalar(name),name=char(name);endif
    if ~ischar(name)||~isrow(name),error('readlines: option names must be text');endif
    if ~strcmpi(name,'EmptyLineRule'),error('readlines: unsupported option %s',name);endif
    value=varargin{k+1};if isa(value,'string')&&isscalar(value),value=char(value);endif
    if ~ischar(value)||~isrow(value)||~any(strcmpi(value,{'read','skip'}))
      error('readlines: EmptyLineRule supports only read or skip');
    endif
    rule=lower(value);
  endfor
endfunction

function filename = local_filename(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('readlines: filename must be a character row vector or string scalar');endif
  filename=value;
endfunction

function local_extension(filename)
  [~,~,ext]=fileparts(filename);ext=lower(ext);
  if ~any(strcmp(ext,{'.csv','.txt','.dat'})),error('readlines: only .csv, .txt and .dat text files are supported');endif
endfunction
