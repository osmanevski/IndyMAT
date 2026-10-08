function s = uy_ozet(x, derinlik)
% UY_OZET Serializer schema 2: class|size|content, column-major values.
% Shared budget per result: 10000 primitive elements/container nodes, depth 8.
% Floating values use 17 significant digits; Python compares finite roundoff.
% Integers use exact decimal tokens (no conversion to double); integer-class
% payloads are compared exactly by scripts/fark.py, without float tolerance.
% <partial:reason> means unmeasured content, never a complete equal result.
% Objects (including table/time types) and struct arrays deliberately remain
% partial: no cross-engine property dispatch/content contract is assumed.
  if nargin < 2, derinlik = 0; end
  [s, ~] = uy_value(x, derinlik, 10000);
end

function [s, budget] = uy_value(x, depth, budget)
  shape = sprintf('%dx', size(x)); shape = shape(1:end-1);
  cls = class(x); head = [cls '|' shape '|'];
  if depth > 8, s = [head '<partial:depth>']; return; end
  if budget <= 0, s = [head '<partial:budget>']; return; end
  budget = budget - 1;
  count = min(numel(x), budget); tail = '';
  if count < numel(x), tail = '<partial:budget>'; end
  if ischar(x)
    budget = budget - count;
    content = ['u' sprintf('%d,', double(x(1:count))) tail];
  elseif isa(x, 'string')
    parts = cell(1, count); budget = budget - count;
    for k = 1:count
      if ismissing(x(k)), parts{k} = '<missing>';
      else, [parts{k}, budget] = uy_value(char(x(k)), depth + 1, budget); end
      if budget <= 0 && k < count
        parts = parts(1:k); tail = '<partial:budget>'; break;
      end
    end
    content = [uy_join(parts) tail];
  elseif islogical(x)
    budget = budget - count;
    content = [sprintf('%d', x(1:count)) tail];
  elseif isnumeric(x)
    budget = budget - count;
    v = x(1:count); parts = cell(1, count);
    for k = 1:count
      if isinteger(x), parts{k} = [uy_integer(v(k)) ','];
      elseif isreal(x), parts{k} = sprintf('%.17g,', double(v(k)));
      else, parts{k} = sprintf('%.17g%+.17gi,', double(real(v(k))), double(imag(v(k)))); end
    end
    content = [parts{:} tail];
  elseif iscell(x)
    parts = cell(1, count);
    for k = 1:count
      [parts{k}, budget] = uy_value(x{k}, depth + 1, budget);
      if budget <= 0 && k < count
        parts = parts(1:k); tail = '<partial:budget>'; break;
      end
    end
    content = [uy_join(parts) tail];
  elseif isstruct(x)
    if numel(x) ~= 1
      content = '<partial:structure-array>';
    else
      fields = sort(fieldnames(x)); count = min(numel(fields), budget);
      parts = cell(1, count); tail = '';
      if count < numel(fields), tail = '<partial:budget>'; end
      for k = 1:count
        [value, budget] = uy_value(x.(fields{k}), depth + 1, budget);
        % Encode field names as character codes to keep marker syntax reserved.
        parts{k} = ['u' sprintf('%d,', double(fields{k})) '=' value];
        if budget <= 0 && k < count
          parts = parts(1:k); tail = '<partial:budget>'; break;
        end
      end
      content = [uy_join(parts) tail];
    end
  else
    % func2str cannot serialize captured workspaces or function identity either.
    content = '<partial:unsupported-object>';
  end
  s = [head content];
end

function text = uy_integer(value)
% Decimal conversion using only integer arithmetic, including int64 minimum
% and uint64 maximum. Only a single remainder digit is converted to double.
  negative = value < 0;
  if negative
    magnitude = uint64(-(int64(value) + int64(1))) + uint64(1);
  else
    magnitude = uint64(value);
  end
  text = '';
  while magnitude > 0
    digit = rem(magnitude, uint64(10));
    text = [char(48 + double(digit)) text];
    magnitude = idivide(magnitude, uint64(10), 'floor');
  end
  if isempty(text), text = '0'; end
  if negative, text = ['-' text]; end
end

function s = uy_join(parts)
  s = '{';
  for k = 1:numel(parts), s = [s parts{k} ';']; end
  s = [s '}'];
end
