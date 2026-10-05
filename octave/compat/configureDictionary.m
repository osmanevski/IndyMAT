function out = configureDictionary (key_type, value_type)
% CONFIGUREDICTIONARY Create an empty dictionary with explicit builtin types.
% Numeric/logical/string keys; numeric/logical/string/cell values only.
% Reconfiguration and user-defined conversion/hash types are unsupported.
  numeric_types = {'double','single','int8','uint8','int16','uint16','int32','uint32','int64','uint64','logical'};
  if (isa (key_type, 'string') && isscalar (key_type)), key_type = char (key_type); endif
  if (isa (value_type, 'string') && isscalar (value_type)), value_type = char (value_type); endif
  if (! ischar (key_type) || ! any (strcmp (key_type, [numeric_types, {'string'}])))
    error ('configureDictionary: unsupported key type');
  endif
  if (! ischar (value_type) || ! any (strcmp (value_type, [numeric_types, {'string','cell'}])))
    error ('configureDictionary: unsupported value type');
  endif
  out = dictionary (empty_value (key_type), empty_value (value_type));
endfunction

function out = empty_value (typ)
  if (strcmp (typ, 'string'))
    if (exist ('string', 'file') != 2), error ('configureDictionary: datatypes string class is required'); endif
    out = string (cell (0, 1));
  elseif (strcmp (typ, 'cell')), out = cell (0, 1);
  else, out = zeros (0, 1, typ); endif
endfunction
