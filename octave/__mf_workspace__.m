function result = __mf_workspace__(mode, argument)
  args = jsondecode(argument);
  result = struct('action', mode);
  if strcmp(mode, 'workspace-rename')
    old_name = __mf_ws_name__(args.old_name);
    new_name = __mf_ws_name__(args.new_name);
    items = __mf_workspace_base__('whos');
    old = items(strcmp({items.name}, old_name));
    if isempty(old), error(__mf_text__('Variable not found: %s', 'Değişken bulunamadı: %s'), old_name); endif
    if old.global, error(__mf_text__('Global variables cannot be renamed in this interface.', 'Global değişkenlerin adı bu arayüzde değiştirilemez.')); endif
    if any(strcmp({items.name}, new_name)), error(__mf_text__('A variable with this name already exists: %s', 'Bu değişken adı zaten var: %s'), new_name); endif
    value = evalin('base', old_name);
    assignin('base', new_name, value);
    try
      __mf_workspace_base__('clear', {old_name});
    catch err
      assignin('base', old_name, value);
      __mf_workspace_base__('clear', {new_name});
      rethrow(err);
    end_try_catch
    result.old_name = old_name;
    result.new_name = new_name;
  elseif strcmp(mode, 'workspace-assign-scalar')
    name = __mf_ws_name__(args.name);
    items = __mf_workspace_base__('whos');
    item = items(strcmp({items.name}, name));
    if isempty(item), error(__mf_text__('Variable not found: %s', 'Değişken bulunamadı: %s'), name); endif
    class_name = item.class;
    if ~strcmp(class_name, args.class) || prod(item.size) ~= 1 || item.complex
      error(__mf_text__('The variable''s class or scalar size has changed; reopen it.', 'Değişkenin sınıfı veya skaler boyutu değişti; yeniden açın.'));
    endif
    if strcmp(class_name, 'logical')
      if ~islogical(args.value) || ~isscalar(args.value), error(__mf_text__('A logical scalar must be true or false.', 'Mantıksal skaler true veya false olmalı.')); endif
      value = args.value;
    elseif strcmp(class_name, 'char')
      if ~ischar(args.value) || numel(args.value) ~= 1 || uint8(args.value) > 127, error(__mf_text__('The character value must be a single ASCII character.', 'Karakter değeri tek ASCII karakteri olmalı.')); endif
      value = args.value;
    elseif any(strcmp(class_name, {'double','single','int8','int16','int32','int64','uint8','uint16','uint32','uint64'}))
      special = isstruct(args.value);
      if special
        if ~any(strcmp(class_name, {'double','single'})) || ~isequal(fieldnames(args.value), {'special'})
          error(__mf_text__('Invalid special floating-point value.', 'Geçersiz özel kayan nokta değeri.'));
        endif
        if strcmp(args.value.special, 'NaN'), number = NaN;
        elseif strcmp(args.value.special, 'Inf'), number = Inf;
        elseif strcmp(args.value.special, '-Inf'), number = -Inf;
        else error(__mf_text__('Invalid special floating-point value.', 'Geçersiz özel kayan nokta değeri.'));
        endif
      else
        number = args.value;
        if ~isnumeric(number) || ~isreal(number) || ~isscalar(number) || ~isfinite(number)
          error(__mf_text__('The numeric value must be a finite scalar or a special-value tag.', 'Sayısal değer sonlu bir skaler veya özel değer etiketi olmalı.'));
        endif
      endif
      value = cast(number, class_name);
      if ~special && ~isfinite(value), error(__mf_text__('The value exceeds the range of its class.', 'Değer sınıf aralığını aşıyor.')); endif
      if isinteger(value) && (abs(number) > 9007199254740991 || double(value) ~= number)
        error(__mf_text__('The integer value must be within the range of its class and exactly representable.', 'Tam sayı değeri sınıf aralığında ve kayıpsız olmalı.'));
      endif
    else
      error(__mf_text__('This scalar class cannot be edited: %s', 'Bu skaler sınıf düzenlenemez: %s'), class_name);
    endif
    assignin('base', name, value);
    result.name = name;
    result.class = class_name;
  elseif strcmp(mode, 'workspace-clear-names')
    names = __mf_ws_names__(args.names);
    if isempty(names), error(__mf_text__('No variables selected to clear.', 'Silinecek değişken seçilmedi.')); endif
    __mf_workspace_base__('clear', names);
    result.names = names;
  elseif strcmp(mode, 'workspace-save')
    % Server-created private staging destination; installation belongs to Python.
    available = __mf_ws_base_names__();
    if args.all
      names = available;
    else
      names = __mf_ws_names__(args.names);
      if any(~ismember(names, available)), error(__mf_text__('One of the selected variables no longer exists.', 'Seçilen değişkenlerden biri artık yok.')); endif
    endif
    if isempty(names), error(__mf_text__('No variables to save.', 'Kaydedilecek değişken yok.')); endif
    values = struct();
    for index = 1:numel(names), values.(names{index}) = evalin('base', names{index}); endfor
    save('-mat7-binary', args.path, '-struct', 'values');
    result.path = args.path;
    result.names = names;
  elseif strcmp(mode, 'workspace-load-inspect')
    items = whos('-file', args.path);
    file_names = __mf_ws_names__({items.name});
    if numel(items) > 5000, error(__mf_text__('The MAT-file contains more than 5000 variables.', 'MAT dosyasında 5000 değişkenden fazlası var.')); endif
    result.path = args.path;
    result.variables = __mf_ws_item_rows__(items);
    result.replacements = intersect(__mf_ws_base_names__(), file_names, 'stable');
  elseif strcmp(mode, 'workspace-load')
    % Read the immutable inspected stage once into helper scope. Validate the
    % complete inventory before assigning ANY field to the base workspace.
    values = load(args.path);
    file_names = __mf_ws_names__(fieldnames(values));
    expected = args.inventory;
    if iscell(expected), expected = [expected{:}]; endif
    if isempty(expected), expected_names = {}; else expected_names = {expected.name}; endif
    if ~isequal(sort(file_names(:)), sort(expected_names(:))), error(__mf_text__('The MAT-file variable list has changed since inspection.', 'MAT değişken listesi incelemeden sonra değişti.')); endif
    for index = 1:numel(expected)
      value = values.(expected(index).name);
      size_text = sprintf('%dx', size(value));
      if ~strcmp(class(value), expected(index).class) || ~strcmp(size_text(1:end-1), expected(index).size)
        error(__mf_text__('A MAT-file variable''s class or size has changed since inspection.', 'MAT değişken sınıfı veya boyutu incelemeden sonra değişti.'));
      endif
    endfor
    replacements = intersect(__mf_ws_base_names__(), file_names, 'stable');
    confirmed = __mf_ws_names__(args.replacements);
    if ~isequal(sort(replacements(:)), sort(confirmed(:))), error(__mf_text__('The variables to replace have changed since confirmation; inspect the file again.', 'Değişecek değişkenler onaydan sonra farklılaştı; dosyayı yeniden inceleyin.')); endif
    for index = 1:numel(file_names), assignin('base', file_names{index}, values.(file_names{index})); endfor
    result.path = args.path;
    result.names = file_names;
    result.replacements = replacements;
  else
    error(__mf_text__('Invalid workspace operation.', 'Geçersiz çalışma alanı işlemi.'));
  endif
endfunction

function name = __mf_ws_name__(name)
  if ~ischar(name) || rows(name) ~= 1 || numel(name) > namelengthmax() || ~isvarname(name) || strncmp(name, '__mf_', 5)
    error(__mf_text__('Invalid or reserved variable name.', 'Geçersiz veya ayrılmış değişken adı.'));
  endif
endfunction

function names = __mf_ws_names__(value)
  if isempty(value), names = {}; return; endif
  if ischar(value), value = {value}; endif
  if ~iscell(value), error(__mf_text__('Variable names must be a list.', 'Değişken adları liste olmalı.')); endif
  names = cell(size(value));
  for index = 1:numel(value), names{index} = __mf_ws_name__(value{index}); endfor
  if numel(unique(names)) ~= numel(names), error(__mf_text__('Variable names must be unique.', 'Değişken adları yinelenemez.')); endif
endfunction

function names = __mf_ws_base_names__()
  items = __mf_workspace_base__('whos');
  names = {items.name};
endfunction

function rows_out = __mf_ws_item_rows__(items)
  rows_out = {};
  for index = 1:numel(items)
    item = items(index);
    size_text = sprintf('%dx', item.size);
    rows_out{end + 1} = struct('name', item.name, 'size', size_text(1:end-1), 'class', item.class, 'bytes', item.bytes);
  endfor
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
