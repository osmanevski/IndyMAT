function __mf_debug_prompt__(path)
  % Readline invokes a builtin-only callback: an m-file/anonymous callback
  % consumes a paused return flag and can itself become a dbstep target.
  % Open the proof channel only inside an attestation, so ordinary fclose all
  % cannot close a descriptor retained across user commands. Trusted user code
  % can alter engine internals; this is an output/protocol boundary, not a sandbox.
  fid = fopen(path, 'w');
  if fid < 0, error(__mf_text__('Could not open debugger prompt channel.', 'Hata ayıklayıcı istem kanalı açılamadı.')); endif
  fclose(fid);
  ticket = '(@getenv)(''INDYMAT_DEBUG_TICKET'')';
  seen = '(@getenv)(''INDYMAT_DEBUG_SEEN'')';
  descriptor = '(@str2double)((@getenv)(''INDYMAT_DEBUG_FID''))';
  quoted_path = ['''' strrep(path, '''', '''''') ''''];
  attest = ['if !(@isempty)(' ticket ') && !(@strcmp)(' ticket ',' seen '); ' ...
    '(@setenv)(''INDYMAT_DEBUG_FID'',(@sprintf)(''%d'',(@fopen)(' quoted_path ',''w''))); ' ...
    'if ' descriptor '>=0; unwind_protect; ' ...
    'if (@fprintf)(' descriptor ',''%s'',' ticket ')==32 && (@fflush)(' descriptor ')==0; ' ...
    '(@setenv)(''INDYMAT_DEBUG_SEEN'',' ticket '); ' ...
    '(@fprintf)(1,''\n__MF_PROMPT_%s__\n'',' ticket '); if (@fflush)(1), endif; ' ...
    'endif; unwind_protect_cleanup; if (@fclose)(' descriptor '), endif; ' ...
    'if (@unsetenv)(''INDYMAT_DEBUG_FID''), endif; end_unwind_protect; endif; endif;'];
  % Idle ticks parse only a short predicate; the channel code is parsed only
  % in debug mode, keeping the always-running hook's CPU cost bounded.
  code = ['unwind_protect; (@eval)(''''); unwind_protect_cleanup; ' ...
    'if (@isdebugmode)(); (@eval)(''' strrep(attest, '''', '''''') '''); endif; end_unwind_protect'];
  add_input_event_hook(@eval, code);
endfunction

function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
