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
    '(@setenv)(''INDYMAT_DEBUG_FID'',(@sprintf)(''%d'',(@fopen)(' quoted_path ',''a''))); ' ...
    'if ' descriptor '>=0; unwind_protect; ' ...
    'if (@fprintf)(' descriptor ',''P %s\n'',' ticket ')==35 && (@fflush)(' descriptor ')==0; ' ...
    '(@setenv)(''INDYMAT_DEBUG_SEEN'',' ticket '); ' ...
    '(@fprintf)(1,''\n__MF_PROMPT_%s__\n'',' ticket '); if (@fflush)(1), endif; ' ...
    'endif; unwind_protect_cleanup; if (@fclose)(' descriptor '), endif; ' ...
    'if (@unsetenv)(''INDYMAT_DEBUG_FID''), endif; end_unwind_protect; endif; endif;'];
  % Debugger readers keep the statement form above: its cleanup block runs the
  % attestation across a pending prompt-level return without consuming it.
  pause_attest = ['unwind_protect; (@eval)(''''); unwind_protect_cleanup; ' ...
    '(@eval)(''' strrep(attest, '''', '''''') '''); end_unwind_protect'];
  % Non-debug user readers also attest after A, for signal-only Stop retries.
  % Idle roots attest once. During Stop an out-of-band flag requests new Qs;
  % the flag grants no permission: Python checks ticket, signal and byte offset.
  % This attestation is one expression, never a statement. With a breakpoint
  % set or a step pending, Octave's evaluator catches an interrupt inside any
  % statement and continues with the next one, so a SIGINT landing in a hook
  % statement never reached the waiting input. An expression has no such
  % catch: the interrupt leaves the hook and aborts the reader, at once or at
  % the next readline tick. A cell literal orders the steps. Between fopen and
  % fclose only calls that never poll for interrupts run (measured), so an
  % interrupt cannot leave a descriptor or an unflushed record behind.
  reader_fid = '(@base64_decode)((@getenv)(''INDYMAT_DEBUG_FID''))';
  wrote = '(@strcmp)((@getenv)(''INDYMAT_DEBUG_WROTE''),''1'')';
  reader_attest = ['(@setenv)(''INDYMAT_ROOT_SEEN'',{' ...
    '(@setenv)(''INDYMAT_DEBUG_RECORD'',(@sprintf)(''Q %s 0 %d\n'',' ticket ',(@numel)((@dbstack)()))),' ...
    '(@setenv)(''INDYMAT_DEBUG_FID'',(@base64_encode)((@double)((@fopen)(' quoted_path ',''a'')))),' ...
    '(@setenv)(''INDYMAT_DEBUG_WROTE'',(@char)(48+(' reader_fid '>=0 && ' ...
      '(@fputs)(' reader_fid ',(@getenv)(''INDYMAT_DEBUG_RECORD''))==0 && (@fflush)(' reader_fid ')==0))),' ...
    reader_fid '>=0 && (@fclose)(' reader_fid ')==0,' ...
    '(@unsetenv)(''INDYMAT_DEBUG_FID''),' ...
    wrote ' && (@fputs)(1,(@sprintf)(''\n__MF_READER_%s__\n'',' ticket '))==0 && (@fflush)(1)==0,' ...
    '{(@getenv)(''INDYMAT_ROOT_SEEN''),' ticket '}{1+(' wrote ' && (@isempty)((@dbstack)()))}}{end})'];
  % The tick itself is one expression too, and Octave parses its whole text on
  % every tick. The branch texts therefore live in environment variables and
  % only this short selector is parsed: a debugger reader takes the pause
  % text, another reader the reader text when it is due, otherwise nothing.
  % The stop flag is tested with stat: exist() searches the whole load path.
  % These variables belong to the bridge like its descriptors; user code that
  % rewrites them manipulates the session and is outside what it defends.
  stop_path = ['''' strrep([path '.stop'], '''', '''''') ''''];
  reader_due = ['(!(@strcmp)(' ticket ',(@getenv)(''INDYMAT_ROOT_SEEN'')) || ' ...
    '!(@isempty)((@stat)(' stop_path ')))'];
  setenv('INDYMAT_HOOK_PAUSE', pause_attest);
  setenv('INDYMAT_HOOK_READER', reader_attest);
  unsetenv('INDYMAT_HOOK_IDLE');
  code = ['(@eval)((@getenv)({''INDYMAT_HOOK_IDLE'',''INDYMAT_HOOK_READER'',''INDYMAT_HOOK_PAUSE'',' ...
    '''INDYMAT_HOOK_PAUSE''}{1+' reader_due '+2*(@isdebugmode)()}))'];
  builtin('add_input_event_hook', @eval, code);
endfunction

function text = __mf_text__(english, turkish)
  text = english;
  if strcmp(getenv('INDYMAT_LANGUAGE'), 'tr'), text = turkish; endif
endfunction
