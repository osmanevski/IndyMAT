"""Bounded MATLAB/native lexical profiles, independently of application state."""
import sys
import time
import unittest
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from backend import source_lexer as lexer


class SourceLexerTests(unittest.TestCase):
    def scan(self, code):
        result = lexer.lex_source(code)
        self.assertTrue(result.ok, result.diagnostics)
        self.assertEqual(''.join(code[t.start:t.end] for t in result.tokens), code)
        return result

    def test_matlab_doubled_quotes_backslashes_apostrophes(self):
        result = self.scan('r="a""b don\'t\\";')
        token = next(t for t in result.tokens if t.kind == 'double')
        self.assertEqual(token.value, 'a"b don\'t\\')
        self.assertEqual(len(token.payload_offsets), len(token.value))

    def test_character_literal_is_not_a_double_literal(self):
        tokens = self.scan("f('it''s \\\" inside')").tokens
        self.assertEqual([t.value for t in tokens if t.kind == 'char'], ['it\'s \\" inside'])
        self.assertFalse(any(t.kind == 'double' for t in tokens))

    def test_transpose_and_matrix_spacing(self):
        for code, count in [("A'", 1), ("A.'", 1), ("r=A ' + B'", 2), ("[A' 'b' \"a\"]", 1), ("{A.' 'b'}", 1), ("A''", 2)]:
            with self.subTest(code=code):
                result = self.scan(code)
                self.assertEqual(sum(t.kind == 'transpose' for t in result.tokens), count)
        self.assertEqual([t.value for t in self.scan("case 'x'").tokens if t.kind == 'char'], ['x'])

    def test_anonymous_field_index_and_control_context(self):
        for code in ['r=@(x) "a";', 'r=@string;', 'r=f(\'a\', "b");', 'r=s.("field");', 'if "a"=="a", r="b"; end', 'for x=1:2, r=["a"; "b"]; end']:
            with self.subTest(code=code): self.scan(code)

    def test_comments_and_continuation(self):
        code = '% "bad\n# \'bad\nr = ["a" ... ignored "\n "b"];'
        result = self.scan(code)
        self.assertEqual([t.value for t in result.tokens if t.kind == 'double'], ['a', 'b'])
        masked = result.masked(code)
        self.assertEqual(len(masked), len(code))
        self.assertEqual(masked.count('\n'), code.count('\n'))
        self.assertNotIn('"', masked)

    def test_nested_whole_line_blocks(self):
        code = '%{\n\n "garbage\n #{\n % hello\n #}\n %{\n%}\n%}\nr="ok";\n'
        self.assertEqual([t.value for t in self.scan(code).tokens if t.kind == 'double'], ['ok'])
        self.scan('r=1; %{ this is a line comment\nr="ok";')
        for code in ['%{\n%}\n%}', '%{\n#}', '#{\n', '%{\n%{\n%}\n']:
            with self.subTest(code=code): self.assertFalse(lexer.lex_source(code).ok)

    def test_command_form_recognition(self):
        for code in ['disp "text"', "disp 'text'", 'disp "text"; r="x";', 'disp -flag', 'name + operand', 'name /path']:
            with self.subTest(code=code): self.assertFalse(lexer.lex_source(code).ok)
        self.assertEqual(lexer.lex_source('disp "x"').diagnostics[0].code, 'quoted-command')
        self.scan('disp("text")')
        self.assertEqual(self.scan('disp text').tokens[0].kind, 'command')

    def test_numeric_elementwise_transpose_and_continuation_longest_match(self):
        result = self.scan("r=1.*A + 2./B + 3.\\C + 4.^D + 5.'; x=1... ignored\n+2;")
        self.assertEqual([t.value for t in result.tokens if t.kind == 'number'], ['1', '2', '3', '4', '5', '1', '2'])
        self.assertEqual(sum(t.kind == 'continuation' for t in result.tokens), 1)
        for code in ['r=1...', 'r=1...\n', 'r=1...\n% comment\n+2;', 'r=1 2;']:
            with self.subTest(code=code): self.assertFalse(lexer.lex_source(code).ok)

    def test_newlines_crlf_and_unicode_content(self):
        code = 'r="ş😀";\r\n% "quotes\r\n\tr="x";\r'
        self.scan(code)
        self.assertEqual([t.value for t in self.scan(code).tokens if t.kind == 'double'], ['ş😀', 'x'])

    def test_longest_operator_match(self):
        tokens = self.scan("r=A.' .* B ./ C .\\ D .^ 2 == E ~= F && G || H;").tokens
        self.assertIn(".'", ["r=A.' .* B ./ C .\\ D .^ 2 == E ~= F && G || H;"[t.start:t.end] for t in tokens])
        self.assertEqual([t.value for t in tokens if t.kind == 'operator'][1:5], ['.*', './', '.\\', '.^'])

    def test_unterminated_cross_line_and_unsupported(self):
        for code in ['r="x', 'r="a\nb";', "r='a\nb';", 'r=("a";', 'r=]', 'arguments\nx\nend', 'classdef X\nend', 'r=0xFFu8;', 'r=1 ! 2', 'r="x" "y"']:
            with self.subTest(code=code): self.assertFalse(lexer.lex_source(code).ok)

    def test_native_double_quote_escape_is_separate(self):
        code = 'r="a\\"b";'
        self.assertFalse(lexer.lex_source(code, 'matlab').ok)
        result = lexer.lex_source(code, 'native-octave')
        self.assertTrue(result.ok, result.diagnostics)
        self.assertFalse(lexer.lex_source(code, 'unknown').ok)

    def test_resource_limits(self):
        self.assertFalse(lexer.lex_source(' ' * (lexer.MAX_BYTES + 1)).ok)
        self.assertFalse(lexer.lex_source('ş' * (lexer.MAX_BYTES // 2 + 1)).ok)
        self.assertFalse(lexer.lex_source('(' * (lexer.MAX_DEPTH + 1)).ok)
        self.assertFalse(lexer.lex_source('r="a"', deadline=time.monotonic() - 1).ok)
        with patch.object(lexer, 'MAX_TOKENS', 2):
            self.assertEqual(lexer.lex_source('r=1;').diagnostics[0].code, 'resource-limit')

    def test_deterministic_tokens(self):
        code = 'r=["a" \"b\"]; % "quote\n'
        self.assertEqual(lexer.lex_source(code), lexer.lex_source(code))


if __name__ == '__main__': unittest.main()
