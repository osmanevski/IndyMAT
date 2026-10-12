function __mf_debug_prompt__(path)
  % Readline invokes a builtin-only callback: an m-file/anonymous callback
  % consumes a paused return flag and can itself become a dbstep target.
  % The dedicated descriptor stays open until process exit. Trusted user code
  % can alter engine internals; this is an output/protocol boundary, not a sandbox.
  fid = fopen(path, 'w+');
  if fid < 0, error(__mf_text__('Could not open debugger prompt channel.', 'Hata ayıklayıcı istem kanalı açılamadı.')); endif
  ticket = '(@getenv)(''INDYMAT_DEBUG_TICKET'')';
  seen = '(@getenv)(''INDYMAT_DEBUG_SEEN'')';
  code = ['unwind_protect; (@eval)(''''); unwind_protect_cleanup; ' ...
    'if (@isdebugmode)() && !(@isempty)(' ticket ') && !(@strcmp)(' ticket ',' seen '); ' ...
    'if (@fseek)(' num2str(fid) ',0,-1)==0 && ' ...
    '(@fprintf)(' num2str(fid) ',''%s'',' ticket ')==32 && (@fflush)(' num2str(fid) ')==0; ' ...
    '(@setenv)(''INDYMAT_DEBUG_SEEN'',' ticket '); ' ...
    '(@fprintf)(1,''\n__MF_PROMPT_%s__\n'',' ticket '); if (@fflush)(1), endif; ' ...
    'endif; endif; end_unwind_protect'];
  add_input_event_hook(@eval, code);
endfunction

function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
