function out = optionalPattern (text)
% OPTIONALPATTERN Greedy zero-or-one occurrence of a scalar ASCII pattern/text.
% Pattern options/arrays are unsupported; nullable extraction/editing/counting
% is rejected, but optional parts inside a mandatory pattern are supported.
  if (nargin != 1), error ('optionalPattern: expected one pattern or scalar text'); endif
  p = pattern (text);
  out = pattern.fromExpression (['(?:' p.expression ')?'], true);
endfunction
