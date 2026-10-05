function __mf_marker__(text)
  % Prints a protocol marker line the kernel's reader looks for.
  mlock();
  fprintf('\n%s\n', text);
  fflush(stdout);
endfunction
