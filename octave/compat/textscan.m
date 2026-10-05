function varargout = textscan (varargin)
  if (! __mf_textscan_pair_form__ (varargin, nargout) && (nargin < 2 || ! ischar (varargin{2}) || isempty (builtin ('regexp', builtin ('regexprep', varargin{2}, '%%', ''), '%[0-9]*c', 'once'))))
    [varargout{1:nargout}] = builtin ('textscan', varargin{:});
    return;
  endif
% TEXTSCAN Character matrices for %c and stop-on-error for a %f%s text pair.
% The builtin parser handles files, widths, limits and options.
% The %f%s subset accepts char sources, a single non-whitespace delimiter,
% optional repeat count, CollectOutput and ReturnOnError. Other numeric
% formats and file sources retain Octave's ReturnOnError behavior.
  if (__mf_textscan_pair_form__ (varargin, nargout))
    [varargout{1:max (1, nargout)}] = __mf_textscan_pair__ (varargin{:});
    return;
  endif
  args = varargin;
  options = 3;
  if (nargin >= 3 && isnumeric (varargin{3})), options = 4; endif
  collect = false;
  for k = options:2:nargin-1
    if (ischar (varargin{k}) && strcmpi (varargin{k}, 'CollectOutput'))
      value = varargin{k+1};
      if ((! islogical (value) && ! isnumeric (value)) || ! isscalar (value))
        [varargout{1:nargout}] = builtin ('textscan', varargin{:});
        return;
      endif
      collect = logical (value);
      args{k+1} = false;
    endif
  endfor
  count = max (1, nargout);
  [varargout{1:count}] = builtin ('textscan', args{:});
  format = builtin ('regexprep', varargin{2}, '%%', '');
  fields = builtin ('regexp', format, '%(\*?)[0-9]*(?:\.[0-9]+)?([a-zA-Z]|\[[^\]]*\])(?:8|16|32|64)?', 'tokens');
  character = [];
  for k = 1:numel (fields)
    if (! isempty (fields{k}{1})), continue; endif
    character(end+1) = strcmp (fields{k}{2}, 'c');
  endfor
  if (numel (character) == numel (varargout{1}))
    for k = find (character)
      if (iscell (varargout{1}{k})), varargout{1}{k} = char (varargout{1}{k}); endif
    endfor
    if (collect)
      values = varargout{1};
      groups = {};
      for k = 1:numel (values)
        if (! isempty (groups) && strcmp (class (values{k}), class (groups{end})))
          groups{end} = [groups{end} values{k}];
        else
          groups{end+1} = values{k};
        endif
      endfor
      varargout{1} = groups;
    endif
  endif
endfunction
