function result = __mf_variable__(mode, argument)
  args = jsondecode(argument);
  name = __mf_var_name__(args.name);
  items = __mf_workspace_base__('whos');
  item = items(strcmp({items.name}, name));
  if isempty(item), error(__mf_text__('Variable not found: %s', 'Değişken bulunamadı: %s'), name); endif
  if numel(item) ~= 1, error(__mf_text__('Variable information is ambiguous.', 'Değişken bilgisi belirsiz.')); endif
  if strcmp(mode, 'variable-read')
    value = evalin('base', name);
    value = __mf_var_follow__(value, args.path);
    result = __mf_var_read__(value, args);
    result.action = 'read';
    result.name = name;
    result.root_class = item.class;
    result.root_size = double(item.size);
    if numel(jsonencode(result)) > 400000, error(__mf_text__('The variable page exceeds the 400 KB transfer limit.', 'Değişken sayfası 400 KB aktarım sınırını aşıyor.')); endif
  elseif strcmp(mode, 'variable-write')
    if ~isempty(args.path), error(__mf_text__('Cell and structure contents are read-only in this version.', 'Hücre ve yapı içeriği bu sürümde salt okunurdur.')); endif
    if ~strcmp(item.class, args.class) || ~isequal(double(item.size(:)'), double(args.size(:)'))
      error(__mf_text__('The variable''s class or size has changed; refresh the page.', 'Değişkenin sınıfı veya boyutu değişti; sayfayı yenileyin.'));
    endif
    value = evalin('base', name);
    if ~isreal(value), error(__mf_text__('Complex arrays are read-only in this version.', 'Karmaşık sayı dizileri bu sürümde salt okunurdur.')); endif
    if ischar(value)
      % A char row is one UTF-8 value, never an independently writable byte.
      replacement = __mf_var_write_text__(value, args);
      assignin('base', name, replacement);
      result = struct('action', 'write', 'name', name, 'class', 'char', 'size', double(size(replacement)));
      return;
    endif
    [subs, replacement] = __mf_var_write_values__(value, args);
    % Typed indices and values never become text, and no temporary variable
    % enters the user's workspace.
    assignin('base', name, subsasgn(value, struct('type', '()', 'subs', {subs}), replacement));
    result = struct('action', 'write', 'name', name, 'class', item.class, 'size', double(item.size));
  else
    error(__mf_text__('Invalid variable operation.', 'Geçersiz değişken işlemi.'));
  endif
endfunction

function name = __mf_var_name__(name)
  if ~ischar(name) || rows(name) ~= 1 || numel(name) > namelengthmax() || ~isvarname(name) || strncmp(name, '__mf_', 5)
    error(__mf_text__('Invalid or reserved variable name.', 'Geçersiz veya ayrılmış değişken adı.'));
  endif
endfunction

function value = __mf_var_follow__(value, path)
  if isempty(path), return; endif
  if ~isstruct(path) && ~iscell(path), error(__mf_text__('Invalid variable path.', 'Değişken yolu geçersiz.')); endif
  if numel(path) > 16, error(__mf_text__('The variable path exceeds the 16-step limit.', 'Değişken yolu 16 adım sınırını aşıyor.')); endif
  for index = 1:numel(path)
    if iscell(path), step = path{index}; else step = path(index); endif
    if ~isstruct(step) || ~isscalar(step), error(__mf_text__('Invalid variable path.', 'Değişken yolu geçersiz.')); endif
    if strcmp(step.kind, 'field')
      if ~isstruct(value) || ~isscalar(value) || ~isfield(value, step.name), error(__mf_text__('The structure field no longer exists.', 'Yapı alanı artık yok.')); endif
      sub = struct('type', '.', 'subs', step.name);
    elseif any(strcmp(step.kind, {'cell','element'}))
      indices = __mf_var_indices__(step.indices, size(value));
      if strcmp(step.kind, 'cell')
        if ~iscell(value), error(__mf_text__('The cell path is no longer valid.', 'Hücre yolu artık geçerli değil.')); endif
        sub = struct('type', '{}', 'subs', {indices});
      else
        if ~isstruct(value), error(__mf_text__('The structure path is no longer valid.', 'Yapı yolu artık geçerli değil.')); endif
        sub = struct('type', '()', 'subs', {indices});
      endif
    else
      error(__mf_text__('Invalid variable path.', 'Değişken yolu geçersiz.'));
    endif
    value = subsref(value, sub);
  endfor
endfunction

function indices = __mf_var_indices__(raw, dimensions)
  raw = double(raw(:)');
  if numel(raw) ~= numel(dimensions) || any(~isfinite(raw)) || any(raw ~= fix(raw)) || any(raw < 1) || any(raw > dimensions)
    error(__mf_text__('Array index exceeds array bounds.', 'Dizi indisi sınır dışı.'));
  endif
  indices = num2cell(raw);
endfunction

function result = __mf_var_read__(value, args)
  dimensions = double(size(value));
  class_name = class(value);
  result = struct('class', class_name, 'size', dimensions, 'real', isreal(value), 'editable', false);
  if ischar(value)
    result = __mf_var_read_text__(value, args, result);
  elseif isnumeric(value) || islogical(value) || iscell(value) || isstruct(value) && ~isscalar(value)
    [subs, row_count, column_count, slices] = __mf_var_page_subs__(dimensions, args);
    page = subsref(value, struct('type', '()', 'subs', {subs}));
    result.row = double(args.row);
    result.column = double(args.column);
    result.row_count = row_count;
    result.column_count = column_count;
    result.row_total = dimensions(1);
    result.column_total = dimensions(2);
    result.slices = slices;
    result.truncated = args.row > 1 || args.column > 1 || args.row + row_count - 1 < dimensions(1) || args.column + column_count - 1 < dimensions(2);
    if isnumeric(value) || islogical(value) || ischar(value)
      result.kind = 'matrix';
      result.rows = __mf_var_matrix_rows__(page);
      result.editable = isempty(args.path) && isreal(value) && any(strcmp(class_name, {'double','single','logical','char','int8','int16','int32','int64','uint8','uint16','uint32','uint64'}));
    else
      result.kind = 'collection';
      result.collection = class_name;
      result.rows = __mf_var_collection_rows__(page, class_name);
    endif
  elseif isstruct(value)
    names = fieldnames(value);
    if numel(names) > 500, error(__mf_text__('The structure exceeds the 500-field display limit.', 'Yapı 500 alan görüntüleme sınırını aşıyor.')); endif
    result.kind = 'struct';
    result.fields = cell(1, numel(names));
    for index = 1:numel(names)
      if numel(names{index}) > 256, error(__mf_text__('The structure field name exceeds the 256-byte limit.', 'Yapı alan adı 256 bayt sınırını aşıyor.')); endif
      field_value = value.(names{index});
      result.fields{index} = struct('name', names{index}, 'display', __mf_var_summary__(field_value), 'class', class(field_value));
    endfor
  else
    result.kind = 'detail';
    % Never invoke arbitrary disp methods or materialise an object's contents.
    result.text = __mf_var_object_summary__(class_name, dimensions);
  endif
  encoded = jsonencode(result);
  if numel(encoded) > 400000, error(__mf_text__('The variable page exceeds the 400 KB transfer limit.', 'Değişken sayfası 400 KB aktarım sınırını aşıyor.')); endif
endfunction

function text = __mf_var_object_summary__(class_name, dimensions)
  shape = sprintf('%dx', dimensions);
  labels = struct('string', __mf_text__('String array', 'Metin dizisi'), 'table', __mf_text__('Table', 'Tablo'), 'datetime', __mf_text__('Datetime array', 'Tarih/saat dizisi'), 'duration', __mf_text__('Duration array', 'Süre dizisi'), 'calendarDuration', __mf_text__('Calendar duration array', 'Takvim süresi dizisi'), 'function_handle', __mf_text__('Function handle', 'Fonksiyon tutamacı'));
  if isfield(labels, class_name), label = labels.(class_name);
  elseif strcmp(class_name, 'containers.Map'), label = __mf_text__('Key/value map', 'Anahtar/değer haritası');
  else label = __mf_text__('Object', 'Nesne');
  endif
  text = sprintf(__mf_text__('%s: %s (%s). Contents are not expanded; the class and size summary is read-only.', '%s: %s (%s). İçerik genişletilmedi; sınıf ve boyut özeti salt okunurdur.'), label, class_name, shape(1:end-1));
endfunction

function [text, valid] = __mf_var_utf8__(bytes)
  text = '';
  valid = false;
  if isempty(bytes), valid = true; return; endif
  try
    text = native2unicode(uint8(bytes), 'UTF-8');
    valid = isequal(uint8(bytes(:)'), uint8(unicode2native(text, 'UTF-8')(:)'));
  catch
    % Invalid bytes must not be silently replaced during JSON encoding.
  end_try_catch
  if ~valid, text = ''; endif
endfunction

function result = __mf_var_read_text__(value, args, result)
  result.kind = 'char-text';
  result.real = true;
  result.text = '';
  result.note = __mf_text__('Character matrices are displayed as complete UTF-8 rows and are read-only.', 'Karakter matrisleri bütün UTF-8 satırlarıyla salt okunur gösterilir.');
  % Bound work BEFORE decoding, concatenating or inspecting entire rows.
  if ndims(value) > 2 || numel(value) > 50000 || rows(value) > 500 || columns(value) > 10000
    result.note = __mf_text__('Text is read-only; limits are 10,000 bytes per complete row, 50,000 bytes total, and 500 rows. Multidimensional character arrays are not expanded.', 'Metin salt okunurdur; tam satır sınırı 10.000 bayt, toplam 50.000 bayt ve 500 satırdır. Çok boyutlu karakter dizileri genişletilmez.');
    return;
  endif
  lines = cell(rows(value), 1);
  counts = zeros(rows(value), 1);
  valid_rows = true(rows(value), 1);
  for index = 1:rows(value)
    bytes = uint8(value(index, :));
    [lines{index}, valid_rows(index)] = __mf_var_utf8__(bytes);
    counts(index) = sum(bitand(bytes, uint8(192)) ~= uint8(128));
    if ~valid_rows(index)
      shown = sprintf('%02X ', bytes(1:min(numel(bytes), 64)));
      lines{index} = [__mf_text__('[Invalid UTF-8; hexadecimal display of up to the first 64 bytes: ', '[Geçersiz UTF-8; ilk 64 bayta kadar onaltılık gösterim: ') strtrim(shown) ']'];
    endif
  endfor
  result.text = strjoin(lines, sprintf('\n'));
  if ~all(valid_rows)
    result.note = __mf_text__('Rows containing invalid UTF-8 are read-only; invalid bytes are displayed in hexadecimal instead of as text.', 'Geçersiz UTF-8 içeren satırlar salt okunurdur; bozuk baytlar metin yerine onaltılık gösterilir.');
  elseif rows(value) <= 1
    result.editable = isempty(args.path);
    result.note = __mf_text__('The row vector is edited as complete UTF-8 text; saving may change its byte length. Limit: 10,000 UTF-8 bytes.', 'Satır vektörü bütün bir UTF-8 metni olarak düzenlenir; kaydetmek bayt uzunluğunu değiştirebilir. Sınır: 10.000 UTF-8 baytı.');
  elseif any(counts ~= counts(1))
    result.note = __mf_text__('Rows have different character counts; the character matrix is read-only.', 'Satırların karakter sayıları eşit değil; karakter matrisi salt okunurdur.');
  endif
endfunction

function replacement = __mf_var_write_text__(value, args)
  if ndims(value) ~= 2 || rows(value) > 1 || numel(value) > 10000
    error(__mf_text__('Only a character row vector of at most 10,000 bytes can be edited.', 'Yalnızca 10.000 baytı aşmayan karakter satır vektörü düzenlenebilir.'));
  endif
  [~, valid] = __mf_var_utf8__(value);
  if ~valid, error(__mf_text__('Text containing invalid UTF-8 is read-only.', 'Geçersiz UTF-8 içeren metin salt okunurdur.')); endif
  if args.row ~= 1 || args.column ~= 1 || args.height ~= 1 || args.width ~= 1 || ~isempty(args.slices) || ~isstruct(args.values) || numel(args.values) ~= 1
    error(__mf_text__('A character row can only be edited as complete text.', 'Karakter satırı yalnızca bütün metin olarak düzenlenebilir.'));
  endif
  item = args.values(1);
  if ~strcmp(item.type, 'text') || ~ischar(item.value) || rows(item.value) > 1 || numel(item.value) > 10000
    error(__mf_text__('A character row must be UTF-8 text of at most 10,000 bytes.', 'Karakter satırı en fazla 10.000 baytlık bir UTF-8 metni olmalı.'));
  endif
  [text, valid] = __mf_var_utf8__(item.value);
  if ~valid, error(__mf_text__('Text must be valid UTF-8.', 'Metin geçerli UTF-8 olmalı.')); endif
  replacement = char(unicode2native(text, 'UTF-8'));
endfunction

function [subs, row_count, column_count, slices] = __mf_var_page_subs__(dimensions, args)
  row = double(args.row);
  column = double(args.column);
  requested_rows = double(args.rows);
  requested_columns = double(args.columns);
  if any(~isfinite([row column requested_rows requested_columns])) || any([row column requested_rows requested_columns] ~= fix([row column requested_rows requested_columns])) || row < 1 || column < 1 || requested_rows < 1 || requested_rows > 100 || requested_columns < 1 || requested_columns > 30 || requested_rows * requested_columns > 3000
    error(__mf_text__('Invalid page bounds.', 'Sayfa sınırları geçersiz.'));
  endif
  if row > max(1, dimensions(1)) || column > max(1, dimensions(2)), error(__mf_text__('The page start exceeds array bounds.', 'Sayfa başlangıcı dizi sınırını aşıyor.')); endif
  slices = double(args.slices(:)');
  if isempty(slices) && numel(dimensions) > 2, slices = ones(1, numel(dimensions) - 2); endif
  if numel(slices) ~= max(0, numel(dimensions) - 2) || any(~isfinite(slices)) || any(slices ~= fix(slices)) || any(slices < 1) || any(slices > dimensions(3:end))
    error(__mf_text__('Slice indices do not match the dimensions.', 'Dilim indisleri boyutlarla eşleşmiyor.'));
  endif
  row_count = min(requested_rows, max(0, dimensions(1) - row + 1));
  column_count = min(requested_columns, max(0, dimensions(2) - column + 1));
  subs = [{row:row + row_count - 1}, {column:column + column_count - 1}, num2cell(slices)];
endfunction

function rows_out = __mf_var_matrix_rows__(page)
  rows_out = cell(rows(page), 1);
  for row = 1:rows(page)
    values = cell(1, columns(page));
    for column = 1:columns(page)
      values{column} = __mf_var_scalar_text__(page(row, column));
    endfor
    rows_out{row} = values;
  endfor
endfunction

function text = __mf_var_scalar_text__(value)
  if ischar(value), text = value; return; endif
  if islogical(value), text = ternary(value, 'true', 'false'); return; endif
  if isinteger(value)
    if value < 0, text = sprintf('%d', value); else text = sprintf('%u', value); endif
    return;
  endif
  precision = ternary(isa(value, 'single'), 9, 17);
  if isreal(value)
    text = __mf_var_real_text__(value, precision);
  else
    real_text = __mf_var_real_text__(real(value), precision);
    imaginary = imag(value);
    sign_text = ternary(signbit(imaginary), '-', '+');
    imaginary_text = __mf_var_real_text__(abs(imaginary), precision);
    text = [real_text sign_text imaginary_text 'i'];
  endif
endfunction

function text = __mf_var_real_text__(value, precision)
  if isnan(value), text = 'NaN';
  elseif isinf(value), text = ternary(value < 0, '-Inf', 'Inf');
  else text = sprintf(['%.' num2str(precision) 'g'], value);
  endif
endfunction

function rows_out = __mf_var_collection_rows__(page, class_name)
  rows_out = cell(rows(page), 1);
  for row = 1:rows(page)
    values = cell(1, columns(page));
    for column = 1:columns(page)
      if strcmp(class_name, 'cell'), value = page{row, column}; else value = page(row, column); endif
      values{column} = struct('display', __mf_var_summary__(value), 'class', class(value));
    endfor
    rows_out{row} = values;
  endfor
endfunction

function text = __mf_var_summary__(value)
  dimensions = size(value);
  shape = sprintf('%dx', dimensions);
  shape = shape(1:end-1);
  if ischar(value) && isrow(value) && numel(value) <= 80
    [decoded, valid] = __mf_var_utf8__(value);
    if valid, text = ['''' decoded '''']; else text = __mf_text__('[Invalid UTF-8; read-only]', '[Geçersiz UTF-8; salt okunur]'); endif
  elseif (isnumeric(value) || islogical(value)) && isscalar(value)
    text = __mf_var_scalar_text__(value);
  else
    text = ['[' shape ' ' class(value) ']'];
  endif
endfunction

function [subs, replacement] = __mf_var_write_values__(value, args)
  dimensions = double(size(value));
  row = double(args.row);
  column = double(args.column);
  height = double(args.height);
  width = double(args.width);
  if any(~isfinite([row column height width])) || any([row column height width] ~= fix([row column height width])) || row < 1 || column < 1 || height < 1 || width < 1 || height > 100 || width > 30 || height * width > 3000
    error(__mf_text__('Invalid write range.', 'Yazma aralığı geçersiz.'));
  endif
  if row + height - 1 > dimensions(1) || column + width - 1 > dimensions(2), error(__mf_text__('The paste exceeds the current array size; this version does not resize arrays.', 'Yapıştırma mevcut dizi boyutunu aşıyor; bu sürüm diziyi büyütmez.')); endif
  slices = double(args.slices(:)');
  if numel(slices) ~= max(0, numel(dimensions) - 2) || any(slices ~= fix(slices)) || any(slices < 1) || any(slices > dimensions(3:end)), error(__mf_text__('Slice indices do not match the dimensions.', 'Dilim indisleri boyutlarla eşleşmiyor.')); endif
  subs = [{row:row + height - 1}, {column:column + width - 1}, num2cell(slices)];
  values = args.values;
  if ~isstruct(values) || numel(values) ~= height * width, error(__mf_text__('Write values do not match the rectangular range.', 'Yazma değerleri dikdörtgen aralıkla eşleşmiyor.')); endif
  class_name = class(value);
  if strcmp(class_name, 'char')
    if height * width ~= 1, error(__mf_text__('Only one cell can be written to a character array.', 'Karakter dizisine yalnızca tek hücre yazılabilir.')); endif
    replacement = __mf_var_typed_scalar__(values(1), class_name);
  else
    replacement = zeros(height, width, class_name);
    for row_index = 1:height
      for column_index = 1:width
        index = (row_index - 1) * width + column_index;
        replacement(row_index, column_index) = __mf_var_typed_scalar__(values(index), class_name);
      endfor
    endfor
  endif
endfunction

function value = __mf_var_typed_scalar__(item, class_name)
  if strcmp(class_name, 'logical')
    if ~strcmp(item.type, 'logical') || ~islogical(item.value) || ~isscalar(item.value), error(__mf_text__('A logical value must be true or false.', 'Mantıksal değer true veya false olmalı.')); endif
    value = item.value;
  elseif strcmp(class_name, 'char')
    if ~strcmp(item.type, 'char') || ~ischar(item.value) || numel(item.value) ~= 1, error(__mf_text__('The character value must be a single character.', 'Karakter değeri tek karakter olmalı.')); endif
    value = item.value;
  elseif any(strcmp(class_name, {'double','single'}))
    if strcmp(item.type, 'special')
      if strcmp(item.value, 'NaN'), number = NaN; elseif strcmp(item.value, 'Inf'), number = Inf; elseif strcmp(item.value, '-Inf'), number = -Inf; elseif strcmp(item.value, '-0'), number = -0.0; else error(__mf_text__('Invalid special floating-point value.', 'Özel kayan nokta değeri geçersiz.')); endif
    elseif strcmp(item.type, 'number') && isnumeric(item.value) && isreal(item.value) && isscalar(item.value) && isfinite(item.value)
      number = item.value;
    else
      error(__mf_text__('Invalid floating-point value.', 'Kayan nokta değeri geçersiz.'));
    endif
    value = cast(number, class_name);
    if isfinite(number) && ~isfinite(value), error(__mf_text__('The value exceeds the range of its class.', 'Değer sınıf aralığını aşıyor.')); endif
  elseif any(strcmp(class_name, {'int8','int16','int32','int64','uint8','uint16','uint32','uint64'}))
    if ~strcmp(item.type, 'integer') || ~ischar(item.value), error(__mf_text__('Invalid integer value.', 'Tam sayı değeri geçersiz.')); endif
    value = __mf_var_integer__(item.value, class_name);
  else
    error(__mf_text__('This array class cannot be edited.', 'Bu dizi sınıfı düzenlenemez.'));
  endif
endfunction

function value = __mf_var_integer__(text, class_name)
  limits = struct('int8', {{'-128','127'}}, 'int16', {{'-32768','32767'}}, 'int32', {{'-2147483648','2147483647'}}, 'int64', {{'-9223372036854775808','9223372036854775807'}}, 'uint8', {{'0','255'}}, 'uint16', {{'0','65535'}}, 'uint32', {{'0','4294967295'}}, 'uint64', {{'0','18446744073709551615'}});
  if isempty(regexp(text, '^-?(0|[1-9][0-9]*)$', 'once')), error(__mf_text__('Invalid integer value.', 'Tam sayı değeri geçersiz.')); endif
  range = limits.(class_name);
  if __mf_var_decimal_compare__(text, range{1}) < 0 || __mf_var_decimal_compare__(text, range{2}) > 0, error(__mf_text__('The %s value exceeds the range of its class.', '%s değeri sınıf aralığını aşıyor.'), class_name); endif
  negative = text(1) == '-';
  digits = text(1 + negative:end);
  magnitude = uint64(0);
  for index = 1:numel(digits), magnitude = magnitude * uint64(10) + uint64(double(digits(index)) - double('0')); endfor
  if negative
    if strcmp(digits, range{1}(2:end)), value = intmin(class_name); else value = -cast(magnitude, class_name); endif
  else
    value = cast(magnitude, class_name);
  endif
endfunction

function comparison = __mf_var_decimal_compare__(left, right)
  left_negative = left(1) == '-'; right_negative = right(1) == '-';
  if left_negative ~= right_negative, comparison = ternary(left_negative, -1, 1); return; endif
  left_digits = left(1 + left_negative:end); right_digits = right(1 + right_negative:end);
  if numel(left_digits) ~= numel(right_digits)
    comparison = sign(numel(left_digits) - numel(right_digits));
  else
    comparison = 0;
    for index = 1:numel(left_digits)
      if left_digits(index) ~= right_digits(index), comparison = sign(double(left_digits(index)) - double(right_digits(index))); break; endif
    endfor
  endif
  if left_negative, comparison = -comparison; endif
endfunction

function value = ternary(condition, yes, no)
  if condition, value = yes; else value = no; endif
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
