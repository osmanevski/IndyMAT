function value = __mf_input_hooks__(action, varargin)
  % Public callbacks keep native ids and map order, but run with this file on
  % their stack. The trusted debugger hook is registered directly with builtin.
  % Native hooks survive clear all; keep their wrapped registry alive too.
  mlock();
  persistent ids = {};
  persistent callbacks = {};
  persistent arguments = {};
  persistent dispatcher = @__mf_input_hooks__;
  persistent dispatcher_id = '';
  persistent warned = false;
  value = [];
  if nargin == 0
    % Native input hooks also tick at root and debugger prompts. Those readers
    % belong to the Kernel protocol. A program input wait retains its caller
    % below this wrapper, including evalin('base',...) inside __mf_execute__.
    if builtin('isdebugmode') || numel(builtin('dbstack')) < 2, return; endif
    % Native iteration chooses the next existing entry before invoking a hook.
    % Look up each callback when reached: replacements/removals take effect
    % during this tick; insertions before the preselected next entry wait.
    if isempty(ids), return; endif
    current = ids{1};
    while !isempty(current)
      index = find(strcmp(ids, current), 1);
      following = find(cellfun(@(id) strcmp(sort({current,id}){1},current) && !strcmp(id,current), ids), 1);
      next = '';
      if !isempty(following), next = ids{following}; endif
      if !isempty(index)
        callback = callbacks{index};
        args = arguments{index};
        if ischar(callback) && isempty(which(callback))
          (@__mf_input_hooks__)('remove', current);
        else
          % eval/assignin/evalin are ordinary documented callbacks too. Calling
          % them in base preserves the workspace used by native input hooks;
          % this file's active frame survives even through evalin('base',...).
          quoted = ['''' strrep(current, '''', '''''') ''''];
          if isempty(args)
            code = ['(@feval)((@__mf_input_hooks__)(''callback'',' quoted '));'];
          else
            code = ['(@feval)((@__mf_input_hooks__)(''callback'',' quoted '),(@__mf_input_hooks__)(''data'',' quoted '));'];
          endif
          evalin('base', code);
        endif
      endif
      current = next;
    endwhile
  elseif strcmp(action, 'add')
    id = varargin{1};
    index = find(strcmp(ids, id), 1);
    if isempty(index), index = numel(ids)+1; endif
    ids{index} = id;
    callbacks{index} = varargin{2};
    arguments{index} = varargin(3:end);
    [ids, order] = sort(ids);
    callbacks = callbacks(order);
    arguments = arguments(order);
    if isempty(dispatcher_id)
      dispatcher_id = builtin('add_input_event_hook', dispatcher);
    endif
    if !warned
      warned = true;
      builtin('warning', 'IndyMAT:inputEventHook:limited', ...
        'Input event hooks run only while a program waits for input().');
    endif
  elseif strcmp(action, 'remove')
    index = find(strcmp(ids, varargin{1}), 1);
    ids(index) = [];
    callbacks(index) = [];
    arguments(index) = [];
    if isempty(ids) && !isempty(dispatcher_id)
      builtin('remove_input_event_hook', dispatcher_id);
      dispatcher_id = '';
    endif
  elseif strcmp(action, 'known')
    value = any(strcmp(ids, varargin{1}));
  elseif strcmp(action, 'callback')
    value = callbacks{find(strcmp(ids,varargin{1}),1)};
  elseif strcmp(action, 'data')
    value = arguments{find(strcmp(ids,varargin{1}),1)}{1};
  endif
endfunction
