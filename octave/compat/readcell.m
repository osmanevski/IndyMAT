function C = readcell(filename, varargin)
% READCELL Read a delimited text file into a cell array.
% Supports .csv, .txt and .dat files plus Delimiter and NumHeaderLines.
% Consumes a leading UTF-8 BOM; detects comma, semicolon, tab, pipe or space
% from up to 32 nonempty records, ignoring delimiters inside quoted fields.
  filename=local_filename(filename);
  [delimiter,header_lines]=local_options(varargin{:});
  local_extension(filename);
  fid=fopen(filename,'rb');
  if fid<0,error('readcell: could not open file %s',filename);endif
  unwind_protect
    text=fread(fid,Inf,'*char')';
  unwind_protect_cleanup
    fclose(fid);
  end_unwind_protect
  if numel(text)>=3&&isequal(uint8(text(1:3)),uint8([239 187 191])),text=text(4:end);endif
  if isempty(delimiter),delimiter=local_detect_delimiter(text,header_lines);endif
  rows=local_parse(text,delimiter);
  if header_lines>=numel(rows),C=cell(0,0);return;endif
  if header_lines>0,rows=rows(header_lines+1:end);endif
  if isempty(rows),C=cell(0,0);return;endif
  nrows=numel(rows);ncols=max(cellfun(@numel,rows));
  C=repmat({missing()},nrows,ncols);
  for r=1:nrows
    for c=1:numel(rows{r})
      token=rows{r}{c};trimmed=strtrim(token);
      if isempty(trimmed),C{r,c}=missing();
      elseif local_is_number(trimmed),C{r,c}=str2double(trimmed);
      else,C{r,c}=token;endif
    endfor
  endfor
endfunction

function filename = local_filename(value)
  if isa(value,'string') && isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('readcell: filename must be a character row vector or string scalar');endif
  filename=value;
endfunction

function local_extension(filename)
  [~,~,ext]=fileparts(filename);ext=lower(ext);
  if ~any(strcmp(ext,{'.csv','.txt','.dat'})),error('readcell: only .csv, .txt and .dat text files are supported');endif
endfunction

function [delimiter,header_lines] = local_options(varargin)
  delimiter='';header_lines=0;
  if mod(numel(varargin),2),error('readcell: options must be name/value pairs');endif
  for k=1:2:numel(varargin)
    name=varargin{k};if isa(name,'string')&&isscalar(name),name=char(name);endif
    if ~ischar(name)||~isrow(name),error('readcell: option names must be text');endif
    switch lower(name)
      case 'delimiter',delimiter=local_delimiter(varargin{k+1});
      case 'numheaderlines'
        value=varargin{k+1};
        if ~isnumeric(value)||~isscalar(value)||~isfinite(value)||value<0||fix(value)~=value
          error('readcell: NumHeaderLines must be a nonnegative integer');
        endif
        header_lines=double(value);
      otherwise,error('readcell: unsupported option %s',name);
    endswitch
  endfor
endfunction

function delimiter = local_delimiter(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('readcell: Delimiter must be text');endif
  switch lower(value)
    case 'comma',delimiter=',';
    case 'tab',delimiter=sprintf('\t');
    case 'space',delimiter=' ';
    case 'semicolon',delimiter=';';
    case 'bar',delimiter='|';
    otherwise
      if numel(value)~=1,error('readcell: only a single-character Delimiter is supported');endif
      delimiter=value;
  endswitch
endfunction

function delimiter = local_detect_delimiter(text,header_lines)
  candidates=[',' ';' sprintf('\t') '|' ' '];
  counts=zeros(0,numel(candidates));row=zeros(1,numel(candidates));
  quoted=false;nonempty=false;record=0;i=1;
  while i<=numel(text)&&rows(counts)<32
    ch=text(i);
    if ch=='"'
      nonempty=true;
      if quoted&&i<numel(text)&&text(i+1)=='"',i=i+1;else,quoted=~quoted;endif
    elseif ~quoted&&(ch==sprintf('\n')||ch==sprintf('\r'))
      record=record+1;
      if record>header_lines&&nonempty,counts(end+1,:)=row;endif
      row(:)=0;nonempty=false;
      if ch==sprintf('\r')&&i<numel(text)&&text(i+1)==sprintf('\n'),i=i+1;endif
    elseif ~quoted
      row=row+(candidates==ch);
      if ~isspace(ch),nonempty=true;endif
    endif
    i=i+1;
  endwhile
  if rows(counts)<32&&record>=header_lines&&nonempty,counts(end+1,:)=row;endif
  delimiter=',';best_support=0;best_width=0;
  for k=1:numel(candidates)
    positive=counts(counts(:,k)>0,k);
    if isempty(positive),continue;endif
    width=mode(positive);support=sum(counts(:,k)==width);
    % Prefer the most consistently repeated column count, then more columns.
    % Candidate order breaks ties; space is last because it also occurs in text.
    if support>best_support||(support==best_support&&width>best_width)
      delimiter=candidates(k);best_support=support;best_width=width;
    endif
  endfor
endfunction

function rows = local_parse(text,delimiter)
  text=strrep(text,sprintf('\r\n'),sprintf('\n'));text=strrep(text,sprintf('\r'),sprintf('\n'));
  rows={};row={};field='';in_quote=false;i=1;
  while i<=numel(text)
    ch=text(i);
    if in_quote
      if ch=='"'
        if i<numel(text)&&text(i+1)=='"',field(end+1)='"';i=i+1;else,in_quote=false;endif
      else,field(end+1)=ch;endif
    elseif ch=='"'&&isempty(field),in_quote=true;
    elseif ch=='"'&&all(isspace(field))
      % R2025b drops whitespace and this opening quote, but does not quote
      % the field: a following delimiter still splits it.
      field='';
    elseif ch==delimiter,row{end+1}=field;field='';
    elseif ch==sprintf('\n'),row{end+1}=field;rows{end+1}=row;row={};field='';
    else,field(end+1)=ch;endif
    i=i+1;
  endwhile
  if in_quote,error('readcell: unterminated quoted field');endif
  if ~isempty(row)||~isempty(field)||(isempty(text)==false&&text(end)~=sprintf('\n'))
    row{end+1}=field;rows{end+1}=row;
  endif
endfunction

function tf = local_is_number(value)
  pattern='^[+-]?((([0-9]+\.?[0-9]*)|(\.[0-9]+))([eE][+-]?[0-9]+)?)$';
  tf=~isempty(regexp(value,pattern,'once'))||~isempty(regexp(value,'^[+-]?(Inf|NaN)$','once'));
endfunction
