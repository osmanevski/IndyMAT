function out = idivide (varargin)
  if (! changed_form (varargin)), original = __mf_original_function__ ('idivide'); out = original (varargin{:}); return; endif
% IDIVIDE Same-class integers: exact fix/floor/ceil without rounded products.
% Includes signed minima and full uint64 precision; division by zero saturates
% like native integer division. Round, mixed double/integer and invalid forms
% delegate unchanged. No floating approximation of 64-bit operands is used.
  a = varargin{1}; b = varargin{2};
  mode = 'fix';
  if (nargin == 3), mode = lower (varargin{3}); endif
  negative = xor (a < 0, b < 0);
  ua = magnitude (a); ub = magnitude (b);
  remainder = rem (ua, ub);
  quotient = (ua - remainder) ./ ub;
  if (strcmp (mode, 'floor'))
    quotient += uint64 (negative & remainder != 0);
  elseif (strcmp (mode, 'ceil'))
    quotient += uint64 (! negative & remainder != 0);
  endif
  out = cast (quotient, class (a));
  if (any (negative(:)))
    % -(q-1)-1 represents intmin; casting q first would saturate to intmax.
    q = quotient(negative);
    out(negative) = -cast (q - uint64 (1), class (a)) - cast (q != 0, class (a));
  endif
endfunction

function ok = changed_form (args)
  ok = numel (args) >= 2 && numel (args) <= 3 && isinteger (args{1}) && isinteger (args{2}) && strcmp (class (args{1}), class (args{2}));
  if (ok && numel (args) == 3)
    ok = ischar (args{3}) && isrow (args{3}) && any (strcmpi (args{3}, {'fix', 'floor', 'ceil'}));
  endif
endfunction

function out = magnitude (x)
  out = uint64 (x);
  negative = x < 0;
  if (any (negative(:)))
    out(negative) = uint64 (-(x(negative) + cast (1, class (x)))) + uint64 (1);
  endif
endfunction
