function __mf_debug_report__()
  % Catch handler for a user line typed at the debug prompt: prints the error
  % message to stderr. The line itself is evaluated at the prompt by a function
  % handle literal, (@eval)(...), so the paused frame sees no helper frame in
  % dbstack, err.stack or warning traces, and return/dbcont keep their meaning.
  mlock();
  fprintf(2, '%s\n', lasterr());
endfunction
