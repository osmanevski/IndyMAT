classdef pattern
% PATTERN Scalar ASCII text patterns, concatenation and ordered alternatives.
% Used with digits/letters/alphanumerics/whitespacePattern and optionalPattern.
% Unicode character classes, pattern arrays, captures and pattern options are
% unsupported. Octave double-quoted literals remain char, not string objects.
  properties (SetAccess = private, Hidden = true)
    expression
    nullable
  endproperties
  methods
    function obj = pattern (text)
      if (nargin != 1), error ('pattern: expected scalar literal text'); endif
      if (isa (text, 'pattern')), obj = text; return; endif
      if (isa (text, 'string'))
        if (! isscalar (text) || ismissing (text)), error ('pattern: expected nonmissing scalar text'); endif
        text = char (text);
      endif
      if (! ischar (text) || (! isrow (text) && ! isempty (text)) || any (uint8 (text) > 127))
        error ('pattern: only ASCII character vectors or scalar strings are supported');
      endif
      obj.expression = regexptranslate ('escape', text);
      obj.nullable = isempty (text);
    endfunction
    function out = plus (a, b)
      a = pattern (a); b = pattern (b);
      out = pattern.fromExpression (['(?:' a.expression ')(?:' b.expression ')'], a.nullable && b.nullable);
    endfunction
    function out = or (a, b)
      a = pattern (a); b = pattern (b);
      out = pattern.fromExpression (['(?:' a.expression '|' b.expression ')'], a.nullable || b.nullable);
    endfunction
    function out = contains (str, pat, varargin)
      out = __mf_pattern_apply__ ('contains', str, pat, varargin{:});
    endfunction
    function out = startsWith (str, pat, varargin)
      out = __mf_pattern_apply__ ('startsWith', str, pat, varargin{:});
    endfunction
    function out = endsWith (str, pat, varargin)
      out = __mf_pattern_apply__ ('endsWith', str, pat, varargin{:});
    endfunction
    function out = count (str, pat, varargin)
      out = __mf_pattern_apply__ ('count', str, pat, varargin{:});
    endfunction
    function out = extract (str, pat)
      out = __mf_pattern_apply__ ('extract', str, pat);
    endfunction
    function out = replace (str, pat, new)
      out = __mf_pattern_apply__ ('replace', str, pat, new);
    endfunction
    function out = insertBefore (str, pat, new)
      out = __mf_pattern_apply__ ('insertBefore', str, pat, new);
    endfunction
    function out = insertAfter (str, pat, new)
      out = __mf_pattern_apply__ ('insertAfter', str, pat, new);
    endfunction
  endmethods
  methods (Static, Hidden = true)
    function out = fromExpression (expression, nullable)
      out = pattern ('');
      out.expression = expression;
      out.nullable = nullable;
    endfunction
  endmethods
endclassdef
