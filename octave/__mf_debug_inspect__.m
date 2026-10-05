function __mf_debug_inspect__(folder, name)
  mlock();
  result = struct('error', '', 'detail', []);
  try
    if !ischar(name) || rows(name) != 1 || numel(name) > namelengthmax() || !isvarname(name) || strncmp(name, '__mf_', 5)
      error(__mf_text__('Invalid or reserved variable name.', 'Geçersiz veya ayrılmış değişken adı.'));
    endif
    value = evalin('caller', name);
    result.detail = __mf_debug_preview__(value, name);
  catch err
    result.error = err.message;
  end_try_catch
  fid = fopen(fullfile(folder, 'debug-inspect.json'), 'w');
  if fid >= 0, fputs(fid, jsonencode(result)); fclose(fid); endif
  [~, job] = fileparts(folder);
  fprintf('\n__MF_DEBUG_INSPECT_%s__\n', job); fflush(stdout);
endfunction

function result = __mf_debug_preview__(value, name)
  result = struct('name', name, 'class', class(value), 'size', size(value), 'rows', {{}}, 'text', '', 'truncated', false);
  if isnumeric(value) || islogical(value)
    if ndims(value) <= 2
      for row_index = 1:min(rows(value), 100)
        row = {};
        for column_index = 1:min(columns(value), 30), row{end + 1} = num2str(value(row_index, column_index), 10); endfor
        result.rows{end + 1} = row;
      endfor
      result.truncated = rows(value) > 100 || columns(value) > 30;
    else
      result.text = sprintf(__mf_text__('%s — multidimensional array; use indexing to inspect it.', '%s — çok boyutlu dizi; indeksleyerek inceleyin.'), mat2str(size(value)));
    endif
  elseif ischar(value)
    shown = value(1:min(rows(value), 100), 1:min(columns(value), 80));
    result.text = strjoin(cellstr(shown), '\n'); result.truncated = rows(value) > 100 || columns(value) > 80;
  elseif iscell(value)
    result.text = sprintf('Cell array %s\n', mat2str(size(value)));
    for index = 1:min(numel(value), 30)
      result.text = [result.text sprintf('{%d}: %s %s\n', index, class(value{index}), mat2str(size(value{index})))];
    endfor
    result.truncated = numel(value) > 30;
  elseif isstruct(value)
    result.text = sprintf(__mf_text__('Struct %s\nFields:\n%s', 'Struct %s\nAlanlar:\n%s'), mat2str(size(value)), strjoin(fieldnames(value), '\n'));
  elseif isa(value, 'function_handle')
    result.text = func2str(value);
  else
    result.text = sprintf('%s %s', class(value), mat2str(size(value)));
  endif
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
