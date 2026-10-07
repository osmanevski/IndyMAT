"""Octave message templates and lexical selectors, using isolated octave-cli jobs."""
import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPERS = ROOT / 'octave'
TURKISH = re.compile('[çğıöşüÇĞİÖŞÜ]')
FORMAT = re.compile(r'%(?:%|[-+ #0]*\d*(?:\.\d+)?[diouxXfFeEgGcs])')


@dataclass
class Token:
    kind: str
    value: str
    start: int
    end: int


def tokens(source):
    """Scan literals, excluding Octave comments and adjacent transpose quotes.

    Keep literal values raw: single-quoted Octave templates retain backslashes,
    and sprintf/error interpret them later. Double quotes support escapes.
    """
    result = []
    pos = 0
    while pos < len(source):
        char = source[pos]
        if char.isspace():
            pos += 1
            continue
        if source.startswith(('%{', '#{'), pos):
            close = '%}' if char == '%' else '#}'
            end = source.find(close, pos + 2)
            if end < 0:
                raise ValueError('Unterminated block comment')
            pos = end + 2
            continue
        if char in '%#' or source.startswith('...', pos):
            end = source.find('\n', pos)
            pos = len(source) if end < 0 else end + 1
            continue
        # A quote adjacent to an identifier, number, close bracket, or dot is
        # a transpose, not a string. Spaced strings can concatenate in matrices.
        previous = result[-1] if result else None
        transpose = char == "'" and previous and previous.end == pos and (
            previous.kind in ('name', 'number') or previous.value in (')', ']', '}', '.', "'"))
        if char in "'\"" and not transpose:
            start = pos
            quote = char
            pos += 1
            value = []
            while pos < len(source):
                char = source[pos]
                if char == quote:
                    if pos + 1 < len(source) and source[pos + 1] == quote:
                        value.append(quote)
                        pos += 2
                        continue
                    pos += 1
                    break
                if quote == '"' and char == '\\' and pos + 1 < len(source):
                    value.append(source[pos:pos + 2])
                    pos += 2
                else:
                    value.append(char)
                    pos += 1
            else:
                raise ValueError(f'Unterminated string at {start}')
            result.append(Token('string', ''.join(value), start, pos))
        elif char.isalpha() or char == '_':
            start = pos
            while pos < len(source) and (source[pos].isalnum() or source[pos] == '_'):
                pos += 1
            result.append(Token('name', source[start:pos], start, pos))
        elif char.isdigit():
            start = pos
            while pos < len(source) and (source[pos].isdigit() or source[pos] == '.'):
                pos += 1
            result.append(Token('number', source[start:pos], start, pos))
        else:
            result.append(Token('symbol', char, pos, pos + 1))
            pos += 1
    return result


def pairs(scanned):
    """Return direct literal template pairs and their Turkish token positions."""
    result = []
    for index, token in enumerate(scanned):
        if token.kind != 'name' or token.value != '__mf_text__':
            continue
        call = scanned[index + 1:index + 6]
        if len(call) == 5 and call[0].value == '(' and call[1].kind == 'string':
            if call[2].value != ',' or call[3].kind != 'string' or call[4].value != ')':
                raise ValueError(f'Expected two literal templates at {token.start}')
            result.append((call[1], call[3]))
    return result


def quote(text):
    return "'" + str(text).replace("'", "''") + "'"


class OctaveMessageSourceTests(unittest.TestCase):
    def test_turkish_literals_only_in_second_template_argument(self):
        count = 0
        for file in sorted(HELPERS.glob('__mf_*.m')):
            source = file.read_text()
            scanned = tokens(source)
            templates = pairs(scanned)
            count += len(templates)
            allowed = {turkish.start for _, turkish in templates}
            for token in scanned:
                if token.kind == 'string' and TURKISH.search(token.value):
                    line = source.count('\n', 0, token.start) + 1
                    self.assertIn(token.start, allowed, f'{file.name}:{line}: {token.value}')
        self.assertGreater(count, 0)

    def test_format_specifiers_match_in_order(self):
        for file in sorted(HELPERS.glob('__mf_*.m')):
            for english, turkish in pairs(tokens(file.read_text())):
                with self.subTest(file=file.name, english=english.value):
                    self.assertEqual(FORMAT.findall(english.value), FORMAT.findall(turkish.value))

    def test_selectors_are_local_and_identical(self):
        standalone = (HELPERS / '__mf_text__.m').read_text()
        selector = standalone[standalone.index('function text = __mf_text__'):]
        for file in sorted(HELPERS.glob('__mf_*.m')):
            source = file.read_text()
            if pairs(tokens(source)):
                self.assertEqual(source.count(selector), 1, file.name)

    def test_scanner_handles_comments_quotes_and_transposes(self):
        source = '''% 'Türkçe comment'
%{
'Türkçe block'
%}
# 'Türkçe comment'
#{
'Türkçe block'
#}
x = value'; y = value.'; z = values(:)';
__mf_text__('Variable''s %s', 'Değişken %s'); % trailing 'yorum'
a = ['Metin' strtrim(x) ']']; b = "Çift # % tırnak";
'''
        scanned = tokens(source)
        self.assertEqual([token.value for token in scanned if token.kind == 'string'],
                         ["Variable's %s", 'Değişken %s', 'Metin', ']', 'Çift # % tırnak'])
        self.assertEqual([(en.value, tr.value) for en, tr in pairs(scanned)],
                         [("Variable's %s", 'Değişken %s')])


@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli is required')
class OctaveMessageRuntimeTests(unittest.TestCase):
    def run_octave(self, body, language='en', setup=None):
        environment = os.environ.copy()
        if language is None:
            environment.pop('INDYMAT_LANGUAGE', None)
        else:
            environment['INDYMAT_LANGUAGE'] = language
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            if setup:
                setup(work)
            script = work / 'i18n_probe.m'
            script.write_text(f'(@addpath)({quote(HELPERS)});\n' + body + '\n')
            result = subprocess.run([shutil.which('octave-cli'), '--quiet', '--no-init-file',
                                     '--no-history', str(script)], cwd=work, env=environment,
                                    text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_selector_language_and_unchanged_templates(self):
        english = 'Value %s %03d %.2f %%'
        turkish = 'Değer %s %03d %.2f %%'
        for language in ('tr', 'en', None, 'garbage', 'TR'):
            with self.subTest(language=language):
                expected = turkish if language == 'tr' else english
                self.run_octave(f'assert(strcmp((@__mf_text__)({quote(english)}, {quote(turkish)}), {quote(expected)}));', language)

    def test_all_templates_return_exact_text(self):
        templates = [pair for file in sorted(HELPERS.glob('__mf_*.m'))
                     for pair in pairs(tokens(file.read_text()))]
        for language in ('tr', 'en'):
            with self.subTest(language=language):
                body = '\n'.join(f'assert(strcmp((@__mf_text__)({quote(en.value)}, {quote(tr.value)}), '
                                 f'{quote(tr.value if language == "tr" else en.value)}));'
                                 for en, tr in templates)
                self.run_octave(body, language)

    def test_real_helper_errors_in_both_languages(self):
        for language in ('tr', 'en'):
            with self.subTest(language=language):
                expected = ['Değişken bulunamadı: missing_value',
                            'Yayın grafik özelliklerinde seyrek diziler desteklenmiyor.'] if language == 'tr' else [
                            'Variable not found: missing_value',
                            'Sparse arrays are not supported in published figure properties.']
                body = r"""
messages = {};
try
  (@__mf_workspace__)('workspace-rename', '{"old_name":"missing_value","new_name":"renamed_value"}');
catch err
  messages{end+1} = err.message;
end_try_catch
try
  (@__mf_publish_equal__)(sparse(1), sparse(1));
catch err
  messages{end+1} = err.message;
end_try_catch
fprintf('__I18N_JSON__%s\n', jsonencode(messages));
"""
                output = self.run_octave(body, language)
                line = next(line for line in output.splitlines() if line.startswith('__I18N_JSON__'))
                self.assertEqual(json.loads(line[len('__I18N_JSON__'):]), expected)

    def test_runtime_switch_and_clear_all_preserve_language_and_user_values(self):
        self.run_octave("""
__mf_text__=731; getenv=732; strcmp=733; kept_value=[4 5 6]; kept_handle=@(x) x+1;
(@setenv)('INDYMAT_LANGUAGE', 'tr');
try
  (@__mf_workspace_base__)('invalid');
catch err
  (@assert)((@strcmp)(err.message, 'Geçersiz çalışma alanı temel işlemi.'));
end_try_catch
(@setenv)('INDYMAT_LANGUAGE', 'en');
try
  (@__mf_workspace_base__)('invalid');
catch err
  (@assert)((@strcmp)(err.message, 'Invalid base workspace operation.'));
end_try_catch
(@assert)(__mf_text__==731 && getenv==732 && strcmp==733);
(@assert)((@isequal)(kept_value,[4 5 6]) && kept_handle(2)==3);
(@setenv)('INDYMAT_LANGUAGE', 'tr');
(@clear)('all');
(@assert)((@strcmp)((@__mf_text__)('English', 'Türkçe'), 'Türkçe'));
try
  (@__mf_workspace_base__)('invalid');
catch err
  (@assert)((@strcmp)(err.message, 'Geçersiz çalışma alanı temel işlemi.'));
end_try_catch
""")

    def test_local_selectors_ignore_shadowing_user_files_and_functions(self):
        def setup(work):
            (work / '__mf_text__.m').write_text("function text=__mf_text__(varargin)\n error('selector hijacked');\nendfunction\n")

        # Exercise non-graphics entry points, including nested publisher dispatch.
        self.run_octave("""
calls = {@() __mf_workspace_base__('invalid'),
         @() __mf_workspace__('invalid', '{}'),
         @() __mf_variable__('variable-read', '{"name":"1invalid"}'),
         @() __mf_publish__('unused.m'),
         @() __mf_publish_equal__(sparse(1), sparse(1)),
         @() __mf_publish_fingerprint_value__(sparse(1))};
mkdir('run-to-cursor.txt');
for kind = 1:2
  if kind == 2
    function text=__mf_text__(varargin)
      error('selector hijacked');
    endfunction
  endif
  for index=1:numel(calls)
    caught=false;
    try
      calls{index}();
    catch err
      caught=true;
      assert(isempty(strfind(err.message, 'selector hijacked')));
      assert(!isempty(strfind(err.message, 'Geçersiz')) || !isempty(strfind(err.message, 'Yayın')));
    end_try_catch
    assert(caught);
  endfor
  tracker=__mf_publish_figures__();
  caught=false;
  try
    tracker('invalid');
  catch err
    caught=true;
    assert(strcmp(err.message, 'Bilinmeyen yayın grafik işlemi.'));
  end_try_catch
  assert(caught);
  data=__mf_figure_data__(0);
  % Figure data v3: the legacy reason is one localized template carrying the stable code.
  assert(strcmp(data.reason, 'Yalnızca PNG görünümü kullanılabilir: no_axes.'));
  assert(strcmp(data.reason_code, 'no_axes'));
  __mf_debug_inspect__(pwd(), '1invalid');
  inspection=jsondecode(fileread('debug-inspect.json'));
  assert(strcmp(inspection.error, 'Geçersiz veya ayrılmış değişken adı.'));
  __mf_run_to_cursor__(pwd(), 'unused.m', 1);
  assert(strcmp(fileread('run-to-cursor-error.txt'), 'Geçici kesme noktası kaydedilemedi.'));
endfor
""", 'tr', setup)

    def test_execute_selector_survives_load_path_removal(self):
        self.run_octave("""
execute=@__mf_execute__;
folder=pwd();
execute(folder, 'snapshot', '');
(@rmpath)(""" + quote(HELPERS) + """);
execute(folder, 'breakpoint', '{"ops":[{"id":"first","code":"error(''first failure'');"},{"id":"second","requires":"first","code":""}]}');
result=jsondecode(fileread('result.json'));
assert(strcmp(result.breakpoint_relocation(2).message, 'Önceki kesme noktası temizlenemediği için yeni konum kurulmadı.'));
""", 'tr')


if __name__ == '__main__':
    unittest.main()
