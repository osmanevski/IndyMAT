function out = insertBefore (str, position, newtext)
% INSERTBEFORE Insert at positions or every non-overlapping literal boundary.
% Char vectors/cellstr retain their types and shape; scalar operands expand.
% String inputs use the datatypes method. Pattern objects, empty text
% boundaries and supplementary Unicode in char/cellstr are unsupported.
  if (nargin != 3), error ('insertBefore: expected three arguments'); endif
  if (isa (str, 'string')), out = str.insertBefore (position, newtext); return; endif
  out = __mf_insert_text__ (str, position, newtext, false, 'insertBefore');
endfunction
