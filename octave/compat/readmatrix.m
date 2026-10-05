function M = readmatrix(filename, varargin)
% READMATRIX Read numeric data from a delimited text file.
% Supports .csv, .txt and .dat files plus Delimiter and NumHeaderLines.
% Missing and nonnumeric fields are returned as NaN.
% BOM removal and quote-aware delimiter detection share readcell's parser.
  filename=local_filename(filename);local_extension(filename);
  [delimiter,header_lines]=local_options(varargin{:});
  args={};if ~isempty(delimiter),args=[args {'Delimiter',delimiter}];endif
  if header_lines,args=[args {'NumHeaderLines',header_lines}];endif
  try,C=readcell(filename,args{:});catch err,error('readmatrix: %s',err.message);end_try_catch
  if isempty(C),M=[];return;endif
  M=NaN(size(C));
  for k=1:numel(C),if isnumeric(C{k})&&isscalar(C{k}),M(k)=double(C{k});endif,endfor
endfunction

function filename = local_filename(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('readmatrix: filename must be a character row vector or string scalar');endif
  filename=value;
endfunction

function local_extension(filename)
  [~,~,ext]=fileparts(filename);ext=lower(ext);
  if ~any(strcmp(ext,{'.csv','.txt','.dat'})),error('readmatrix: only .csv, .txt and .dat text files are supported');endif
endfunction

function [delimiter,header_lines] = local_options(varargin)
  delimiter='';header_lines=0;
  if mod(numel(varargin),2),error('readmatrix: options must be name/value pairs');endif
  for k=1:2:numel(varargin)
    name=varargin{k};if isa(name,'string')&&isscalar(name),name=char(name);endif
    if ~ischar(name)||~isrow(name),error('readmatrix: option names must be text');endif
    switch lower(name)
      case 'delimiter',delimiter=local_delimiter(varargin{k+1});
      case 'numheaderlines'
        value=varargin{k+1};
        if ~isnumeric(value)||~isscalar(value)||~isfinite(value)||value<0||fix(value)~=value,error('readmatrix: NumHeaderLines must be a nonnegative integer');endif
        header_lines=double(value);
      otherwise,error('readmatrix: unsupported option %s',name);
    endswitch
  endfor
endfunction

function delimiter = local_delimiter(value)
  if isa(value,'string')&&isscalar(value),value=char(value);endif
  if ~ischar(value)||~isrow(value)||isempty(value),error('readmatrix: Delimiter must be text');endif
  switch lower(value)
    case 'comma',delimiter=',';case 'tab',delimiter=sprintf('\t');case 'space',delimiter=' ';
    case 'semicolon',delimiter=';';case 'bar',delimiter='|';
    otherwise,if numel(value)~=1,error('readmatrix: only a single-character Delimiter is supported');endif;delimiter=value;
  endswitch
endfunction
