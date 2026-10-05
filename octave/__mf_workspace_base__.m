function items = __mf_workspace_base__(operation, names = {})
  % Base-workspace queries use function-handle literals, which resolve the
  % function even when a user variable has its name, and need no temporary
  % variable in the user's workspace.
  items = [];
  if strcmp(operation, 'whos')
    items = evalin('base', '(@whos)()');
  elseif strcmp(operation, 'clear')
    quoted = cellfun(@(name) ["'" strrep(name, "'", "''") "'"], names, 'UniformOutput', false);
    if !isempty(quoted)
      evalin('base', ['(@clear)(' strjoin(quoted, ',') ');']);
    endif
  else
    error(__mf_text__('Invalid base workspace operation.', 'Geçersiz çalışma alanı temel işlemi.'));
  endif
  if isstruct(items)
    items = items(~strncmp({items.name}, '__mf_', 5));
  endif
endfunction

% Local selector: lexical lookup survives path removal and user functions
% named __mf_text__; caller/base variables cannot enter this workspace.
function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
