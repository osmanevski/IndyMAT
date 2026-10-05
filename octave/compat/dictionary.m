classdef dictionary
% DICTIONARY Ordered scalar value container; numeric/logical/string keys.
% Character vectors normalize to package string keys/values; cell keys are unsupported.
% Numeric/logical/string/cell values; one-dimensional key-based () access.
% No object arrays, end/brace indexing, chained assignment, implicit numeric
% type conversion, entries tables or unconfigured types/keys/values yet.
  properties (Access = private)
    key_data = {}
    value_data = {}
    key_type = ''
    value_type = ''
  endproperties
  methods
    function obj = dictionary (varargin)
      if (mod (nargin, 2)), error ('dictionary: expected key/value pairs'); endif
      for k = 1:2:nargin
        obj = assign (obj, varargin{k}, varargin{k+1});
      endfor
    endfunction

    function n = numEntries (obj)
      n = numel (obj.key_data);
    endfunction

    function out = isConfigured (obj)
      out = ! isempty (obj.key_type);
    endfunction

    function [kt, vt] = types (obj)
      if (! isConfigured (obj)), error ('dictionary: types of an unconfigured dictionary are unsupported pending MATLAB measurement'); endif
      require_string ();
      kt = string (obj.key_type); vt = string (obj.value_type);
    endfunction

    function out = keys (obj, varargin)
      if (! isempty (varargin)), error ('dictionary: keys output options are unsupported'); endif
      if (! isConfigured (obj)), error ('dictionary: keys of an unconfigured dictionary are unsupported pending MATLAB measurement'); endif
      out = pack (obj.key_data, obj.key_type, [numEntries(obj), 1]);
    endfunction

    function out = values (obj, varargin)
      if (! isempty (varargin)), error ('dictionary: values output options are unsupported'); endif
      if (! isConfigured (obj)), error ('dictionary: values of an unconfigured dictionary are unsupported pending MATLAB measurement'); endif
      out = pack (obj.value_data, obj.value_type, [numEntries(obj), 1]);
    endfunction

    function out = entries (obj, varargin)
      error ('dictionary: entries table shape requires MATLAB measurement; use keys and values');
    endfunction

    function out = isKey (obj, query)
      [items, typ, shape] = unpack (query, true);
      check_key_type (obj, typ);
      out = false (shape);
      for k = 1:numel (items), out(k) = find_key (obj, items{k}) != 0; endfor
    endfunction

    function out = lookup (obj, query, varargin)
      [items, typ, shape] = unpack (query, true);
      check_key_type (obj, typ);
      fallback = {}; have_fallback = ! isempty (varargin);
      if (have_fallback)
        if (numel (varargin) != 2 || ! ischar (varargin{1}) || ! strcmp (varargin{1}, 'FallbackValue'))
          error ('dictionary: lookup supports only FallbackValue');
        endif
        [fallback, vt] = unpack (varargin{2}, false);
        if (numel (fallback) != 1 || ! strcmp (vt, obj.value_type))
          error ('dictionary: FallbackValue must be a scalar of the configured value type');
        endif
      endif
      found = cell (1, numel (items));
      for k = 1:numel (items)
        index = find_key (obj, items{k});
        if (index == 0)
          if (! have_fallback)
            if (numel (items) == 1), error ('MATLAB:dictionary:ScalarKeyNotFound', 'Key not found.');
            else, error ('MATLAB:dictionary:KeyNotFound', 'Element %d of the key array not found.', k); endif
          endif
          found{k} = fallback{1};
        else, found{k} = obj.value_data{index}; endif
      endfor
      out = pack (found, obj.value_type, shape);
    endfunction

    function obj = insert (obj, query, val, varargin)
      if (! isempty (varargin)), error ('dictionary: insert options are unsupported'); endif
      obj = assign (obj, query, val);
    endfunction

    function obj = remove (obj, query)
      [items, typ] = unpack (query, true);
      check_key_type (obj, typ);
      for k = 1:numel (items)
        index = find_key (obj, items{k});
        if (index == 0), error ('dictionary: removing absent keys is unsupported pending MATLAB measurement'); endif
        obj.key_data(index) = []; obj.value_data(index) = [];
      endfor
    endfunction

    function varargout = subsref (obj, s)
      if (strcmp (s(1).type, '()'))
        if (numel (s(1).subs) != 1), error ('dictionary: exactly one key array is required'); endif
        reject_colon (s(1).subs{1});
        out = lookup (obj, s(1).subs{1});
        if (numel (s) > 1), out = subsref (out, s(2:end)); endif
        varargout{1} = out;
      elseif (strcmp (s(1).type, '.'))
        [varargout{1:nargout}] = builtin ('subsref', obj, s);
      else, error ('dictionary: brace indexing is unsupported'); endif
    endfunction

    function obj = subsasgn (obj, s, val)
      if (! strcmp (s(1).type, '()') || numel (s) != 1 || numel (s(1).subs) != 1)
        error ('dictionary: only direct assignment to one key array is supported');
      endif
      reject_colon (s(1).subs{1});
      if (isnumeric (val) && isempty (val)), obj = remove (obj, s(1).subs{1});
      else, obj = assign (obj, s(1).subs{1}, val); endif
    endfunction

    function n = numel (obj, varargin)
      if (! isempty (varargin) || builtin ('numel', obj) != 1), error ('dictionary: object-array indexing is unsupported'); endif
      n = 1;
    endfunction

    function n = end (obj, varargin)
      error ('dictionary: end indexing is unsupported; index by a key');
    endfunction

    function out = horzcat (varargin)
      error ('dictionary: object arrays are unsupported; use a cell array');
    endfunction

    function out = vertcat (varargin)
      error ('dictionary: object arrays are unsupported; use a cell array');
    endfunction

    function out = repmat (varargin)
      error ('dictionary: object arrays are unsupported; use a cell array');
    endfunction

    function out = reshape (varargin)
      error ('dictionary: object reshaping is unsupported');
    endfunction

    function disp (obj)
      if (isConfigured (obj))
        fprintf ('  dictionary (%s -> %s) with %d entries\n', obj.key_type, obj.value_type, numEntries (obj));
      else, fprintf ('  unconfigured dictionary\n'); endif
    endfunction

    function display (obj)
      disp (obj);
    endfunction
  endmethods
  methods (Access = private)
    function obj = assign (obj, query, val)
      [ks, kt, key_shape] = unpack (query, true); [vs, vt, value_shape] = unpack (val, false);
      if (! isConfigured (obj)), obj.key_type = kt; obj.value_type = vt;
      elseif (! strcmp (kt, obj.key_type) || ! strcmp (vt, obj.value_type))
        error ('dictionary: implicit type conversions are unsupported; use the configured key and value types');
      endif
      if (numel (vs) == 1), vs = repmat (vs, 1, numel (ks));
      elseif (! isequal (key_shape, value_shape))
        error ('dictionary: keys and values must have matching dimensions unless values are scalar');
      endif
      for k = 1:numel (ks)
        index = find_key (obj, ks{k});
        if (index == 0), index = numEntries (obj) + 1; obj.key_data{index} = ks{k}; endif
        obj.value_data{index} = vs{k};
      endfor
    endfunction

    function check_key_type (obj, typ)
      if (! isConfigured (obj)), error ('dictionary: querying an unconfigured dictionary is unsupported pending MATLAB measurement'); endif
      if (! strcmp (typ, obj.key_type)), error ('dictionary: implicit key type conversion is unsupported'); endif
    endfunction

    function index = find_key (obj, key)
      index = 0;
      for k = 1:numel (obj.key_data)
        if (isequaln (obj.key_data{k}, key)), index = k; return; endif
      endfor
    endfunction
  endmethods
endclassdef

function [items, typ, shape] = unpack (x, is_key)
  typ = class (x); shape = size (x);
  if (ischar (x))
    if (! isrow (x) && ! isempty (x)), error ('dictionary: character matrices are unsupported'); endif
    require_string (); items = {x}; typ = 'string'; shape = [1 1];
  elseif (isa (x, 'string'))
    if (any (ismissing (x)(:))), error ('dictionary: missing string values are unsupported'); endif
    items = cellstr (x); items = items(:).';
  elseif (iscell (x))
    if (is_key)
      error ('dictionary: cell keys are unsupported (MATLAB keeps them as cell keys); use string keys, e.g. string({''a'',''b''})');
    endif
    items = x(:).';
  elseif (isnumeric (x) || islogical (x))
    if (is_key && (! isreal (x) || any (! isfinite (x(:)))))
      error ('dictionary: complex and nonfinite numeric keys require MATLAB measurement');
    endif
    items = num2cell (x(:).');
  else, error ('dictionary: unsupported key or value class %s', typ); endif
endfunction

function out = pack (items, typ, shape)
  if (strcmp (typ, 'string'))
    require_string (); out = reshape (string (reshape (items, shape)), shape);
  elseif (strcmp (typ, 'cell')), out = reshape (items, shape);
  else
    out = zeros (shape, typ);
    for k = 1:numel (items), out(k) = items{k}; endfor
  endif
endfunction

function require_string ()
  if (exist ('string', 'file') != 2), error ('dictionary: the datatypes string class is required for text'); endif
endfunction

function reject_colon (query)
  if (ischar (query) && strcmp (query, ':'))
    error ('dictionary: colon indexing is unsupported; use string('':'') for a literal colon key');
  endif
endfunction
