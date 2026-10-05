"""Production literal rewrite, source maps and isolated octave-cli verification."""
import dataclasses
import hashlib
import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from backend.source_adapter import (ADAPTER_VERSION, AdapterProfile, PackageSupport, SourceSpan,
                                    STRING_AWARE, SPRINTF_EVIDENCE, adapt_source, verify_package)
from scripts import compat, fark

# Unit lexer/map tests do not require an installed engine. The real engine tests
# obtain their own support by resolving and hash-checking the actual constructor.
UNIT_PROFILE = AdapterProfile(True, package=PackageSupport(True, '/verified/string.m', 'unit-fixture', ''))


class SourceAdapterTests(unittest.TestCase):
    def adapted(self, source, span=None):
        result = adapt_source(source, span, UNIT_PROFILE)
        self.assertEqual(result.status, 'adapted', result.diagnostics)
        self.assertEqual(result.adapter_version, ADAPTER_VERSION)
        self.assertEqual(result.generated_text.count('\n'), (source if span is None else source[span[0]:span[1]]).count('\n'))
        return result

    def test_quote_doubling_apostrophes_and_trailing_backslashes(self):
        pairs = [('""', "(@string)('')"), ('"abc"', "(@string)('abc')"),
                 ('"a""b"', "(@string)('a\"b')"), ('"don\'t"', "(@string)('don''t')"),
                 ('"a\\nb"', "(@string)('a\\nb')"), ('"tail\\"', "(@string)('tail\\')"),
                 ('"% # ..."', "(@string)('% # ...')")]
        for before, after in pairs:
            with self.subTest(before=before): self.assertEqual(self.adapted(before).generated_text, after)

    def test_transpose_char_comments_and_continuations_untouched(self):
        code = "r = A' + A.'; c='it''s \"text\"'; % \"bad\nr=[\"a\" ... \"ignored\n \"b\"]; # \"bad\n"
        result = self.adapted(code)
        self.assertEqual(len(result.replacements), 2)
        self.assertEqual(result.generated_text, code.replace('"a"', "(@string)('a')").replace('"b"', "(@string)('b')"))

    def test_nested_blocks_crlf_matrix_and_cell_spacing(self):
        code = '%{\r\n #{\r\n "bad\r\n #}\r\n%}\r\nr=["a" \'b\'; "c" "d"]; s={"e", \'f\'};\r\n'
        result = self.adapted(code)
        self.assertEqual(len(result.replacements), 4)
        self.assertEqual(result.generated_text.count('\r\n'), code.count('\r\n'))
        self.assertIn("[(@string)('a') 'b';", result.generated_text)
        self.assertIn("{(@string)('e'), 'f'}", result.generated_text)

    def test_default_disabled_and_native_profile(self):
        source = 'r="a\\n";'
        self.assertEqual(adapt_source(source).status, 'unchanged')
        native = dataclasses.replace(UNIT_PROFILE, dialect='native-octave')
        self.assertEqual(adapt_source(source, None, native).generated_text, source)
        self.assertEqual(adapt_source('r=1', None, UNIT_PROFILE).status, 'unchanged')

    def test_whole_unit_fallback_never_partial(self):
        for source in ['r="a"; disp "b"', 'r="a"; x="broken', 'r="a"; x="cross\nline"',
                       'r="a"; name -flag', 'r="a"; arguments\nend', 'r="a"; x=(1',
                       'r="a"; x=0xFFu8;', 'r="a"; x=1+', '%{\n "a"']:
            with self.subTest(source=source):
                result = adapt_source(source, None, UNIT_PROFILE)
                self.assertEqual(result.status, 'fallback')
                self.assertEqual(result.generated_text, source)
                self.assertEqual(result.replacements, ())
                self.assertTrue(result.diagnostics)
        self.assertEqual(self.adapted('disp("b")').generated_text, "disp('b')")

    def test_mid_line_selection_and_whole_document_context(self):
        source = 'x=1; r="ab"; y=2;'
        result = self.adapted(source, (5, 12))
        self.assertEqual(result.generated_text, "r=(@string)('ab');")
        self.assertEqual(result.replacements[0].original, SourceSpan(7, 11))
        for source, span in [('r="abc";', (4, 6)), ('% "abc"\nr="x";', (2, 7)),
                             ("c='\"abc\"'; r=1;", (3, 8)), ('r=func("x");', (3, 12)),
                             ('r=("x");', (2, 6)), ('r="x"; % comment', (12, 16))]:
            with self.subTest(source=source, span=span):
                result = adapt_source(source, span, UNIT_PROFILE)
                self.assertEqual(result.status, 'fallback', result)
                self.assertEqual(result.generated_text, source[span[0]:span[1]])

    def test_selection_splitting_crlf_falls_back(self):
        source = 'r=1;\r\nr="x";'
        result = adapt_source(source, (5, len(source)), UNIT_PROFILE)
        self.assertEqual(result.status, 'fallback')
        self.assertEqual(result.generated_text, source[5:])

    def test_utf16_selection_utf8_map_and_scaffolding(self):
        source = '% 😀\r\nr="ş😀a""b\'c"; tail=1;'
        start = len('% 😀\r\n'.encode('utf-16-le')) // 2
        result = adapt_source(source, (start, len(source.encode('utf-16-le')) // 2), UNIT_PROFILE)
        self.assertEqual(result.status, 'adapted')
        self.assertEqual(result.replacements[0].original.start_utf16, start + 2)
        scaffold = next(p for p in result.source_map.pieces if p.kind == 'scaffolding')
        mapped, adapted = result.source_map.locate(scaffold.generated_start_utf16)
        self.assertEqual(mapped, result.replacements[0].original)
        self.assertTrue(adapted)
        emoji = next(p for p in result.source_map.pieces if p.kind == 'payload' and p.generated_end_utf8 - p.generated_start_utf8 == 4)
        self.assertEqual(emoji.original_end_utf16 - emoji.original_start_utf16, 2)
        self.assertEqual(result.source_map.locate(emoji.generated_start_utf8, units='utf8')[0], SourceSpan(emoji.original_start_utf16, emoji.original_end_utf16))
        doubled_quote = next(p for p in result.source_map.pieces if p.kind == 'payload' and p.original_end_utf16 - p.original_start_utf16 == 2 and p.generated_end_utf16 - p.generated_start_utf16 == 1)
        self.assertEqual(doubled_quote.original_line, 2)
        tail = result.source_map.pieces[-1]
        self.assertEqual(tail.kind, 'exact')
        self.assertEqual(result.source_map.locate(tail.generated_start_utf16)[0].start_utf16, tail.original_start_utf16)
        with self.assertRaises(ValueError): result.source_map.locate(0, units='octave-column')

    def test_exact_maps_tabs_newlines_and_supplementary_unicode(self):
        source = '% 😀\r\n\tr="x";\rnext=1;'
        result = self.adapted(source)
        for piece in result.source_map.pieces:
            if piece.kind == 'exact':
                self.assertEqual(result.source_map.locate(piece.generated_start_utf8, units='utf8'), (SourceSpan(piece.original_start_utf16, piece.original_end_utf16), False))
        following = next(p for p in result.source_map.pieces if p.kind == 'exact' and p.original_line == 3)
        self.assertEqual(following.generated_line, 3)
        self.assertEqual(following.generated_column_utf16, 1)

    def test_invalid_utf16_and_invalid_inputs(self):
        for span in [(0,), (0, 1, 2), (-1, 1), (2, 1), (0, 100), (1.0, 2), (1, 2)]:
            with self.subTest(span=span), self.assertRaises(ValueError): adapt_source('😀"x"', span, UNIT_PROFILE)
        with self.assertRaises(ValueError): adapt_source('r="x"', None, 'matlab')
        with self.assertRaises(ValueError): adapt_source('\ud800', None, UNIT_PROFILE)

    def test_no_package_and_unsupported_package_fall_back(self):
        result = adapt_source('r="a";', None, AdapterProfile(True))
        self.assertEqual(result.status, 'fallback')
        self.assertEqual(result.diagnostics[0].code, 'package-unverified')
        for code in ['string.m', 'missing']:
            with tempfile.TemporaryDirectory() as temp:
                path = Path(temp, code)
                if code == 'string.m': path.write_text('unpatched')
                self.assertFalse(verify_package(path, executable='octave-cli').verified)
        self.assertFalse(verify_package('/missing', executable='octave').verified)

    def test_resource_fallback_and_bounded_map(self):
        result = adapt_source('r="x"; ' * 18000, None, UNIT_PROFILE)
        self.assertEqual(result.status, 'fallback')
        self.assertEqual(result.replacements, ())
        self.assertLessEqual(len(result.source_map.pieces), 100000)
        with self.assertRaises(ValueError): adapt_source(' ' * 500001, None, UNIT_PROFILE)

    def test_snapshot_immutability_without_application(self):
        # A dirty buffer is text. Retaining an adaptation of an old snapshot
        # does not consult/rewrite a saved file or a subsequently edited buffer.
        first = self.adapted('r="old";')
        second = self.adapted('r="edited";')
        self.assertEqual(first.generated_text, "r=(@string)('old');")
        self.assertNotEqual(first.generated_text, second.generated_text)
        self.assertEqual(first.replacements[0].original_text, '"old"')

    def test_limitations_visible_and_only_direct_text_literals_lowered(self):
        result = self.adapted('r=sprintf("%s", "x"); s=eval("a=1");')
        self.assertTrue(any(d.code == 'semantic-limit' for d in result.diagnostics))
        self.assertNotIn('(@char)', result.generated_text)
        self.assertIn("eval('a=1')", result.generated_text)
        self.assertIn("(@string)(sprintf('%s', 'x'))", result.generated_text)
        result = self.adapted('r="😀";')
        self.assertTrue(any(d.code == 'unicode-limit' for d in result.diagnostics))

    def test_direct_flags_preserve_matlab_payload_semantics(self):
        code = 'r=struct("don\'t", "a\\n"); x=interp1(a,b,c,"linear");'
        result = self.adapted(code)
        self.assertEqual(result.generated_text, "r=struct('don''t', 'a\\n'); x=interp1(a,b,c,'linear');")
        payload = next(p for p in result.source_map.pieces if p.kind == 'payload' and p.generated_end_utf16 - p.generated_start_utf16 == 2)
        self.assertEqual(payload.original_end_utf16 - payload.original_start_utf16, 1)

    def test_objects_in_operators_arrays_cells_and_aware_arguments(self):
        result = self.adapted('a="x"; b=["y" "z"]; c={"q"}; r=split("a,b",",");')
        self.assertIn("a=(@string)('x')", result.generated_text)
        self.assertIn("[(@string)('y') (@string)('z')]", result.generated_text)
        self.assertIn("{(@string)('q')}", result.generated_text)
        self.assertIn("split((@string)('a,b'),(@string)(','))", result.generated_text)
        self.assertIn("str2double({(@string)('2')})", self.adapted('r=str2double({"2"});').generated_text)

    def test_unsupported_argument_contexts_fall_back_as_whole_units(self):
        cases = {
            'r="ok"; r=bin2dec(["1";"10"]);': 'string-array-argument',
            'r=sum("a"+"b");': 'string-expression-argument',
            'r=sum(("all"));': 'string-expression-argument',
            'r=custom("a");': 'unresolved-string-call',
            'r=(@sum)(a,"all");': 'unresolved-string-call',
            'h=@sum; r=h(a,"all");': 'unresolved-string-call',
            'r=s.f("x");': 'unresolved-string-call',
            'r=s.("field");': 'unresolved-string-call',
            'sum=1:3; r=sum("x");': 'unresolved-string-call',
            'sum(1)=1; r=sum("x");': 'unresolved-string-call',
            'r=@(sum) sum("x");': 'unresolved-string-call',
            '[sum,x]=deal(1,2); r=sum("x");': 'unresolved-string-call',
            's="all"; r=sum(x,s);': 'string-forwarding',
            's="all"; q=s; r=sum(x,q);': 'string-forwarding',
            'fmt="%d"; r=sprintf(fmt,2);': 'string-forwarding',
            'r=disp(sprintf("%d",2));': 'string-result-argument',
            's="x"; d.Format=s;': 'string-property-write',
            'd.Format="x";': 'string-property-write',
            'd.Format(1)="x";': 'string-property-write',
            'for x=["a" "b"], r=x; end': 'string-iteration',
            's=["a" "b"]; for x=s, r=x; end': 'string-iteration',
        }
        for code, diagnostic in cases.items():
            with self.subTest(code=code):
                result = adapt_source(code, None, UNIT_PROFILE)
                self.assertEqual(result.status, 'fallback', result)
                self.assertEqual(result.generated_text, code)
                self.assertEqual(result.replacements, ())
                self.assertEqual(result.diagnostics[0].code, diagnostic)

    def test_numeric_loops_and_char_converters_keep_context(self):
        self.adapted('for i=1:2, r="x"; end')
        self.adapted('s="x"; d.Field=char(s);')
        self.adapted('s="x"; r=disp(char(s));')
        self.adapted('s="x"; r=unknown(double("123"));')
        self.adapted('r=disp(sprintf(\'%s\',"x"));')

    def test_sprintf_wrapper_spans_maps_and_nested_calls(self):
        code = 'r=string(sprintf("%s:%d", "ş\'x", 2));\r\n'
        result = self.adapted(code)
        self.assertEqual(result.generated_text, "r=string((@string)(sprintf('%s:%d', 'ş''x', 2)));\r\n")
        prefix = next(p for p in result.source_map.pieces if p.kind == 'scaffolding' and p.generated_end_utf16 - p.generated_start_utf16 == len('(@string)('))
        self.assertEqual(result.source_map.locate(prefix.generated_start_utf16)[0], SourceSpan(code.index('sprintf'), code.index('));') + 1))
        # Applying the public replacement spans reproduces the generated text,
        # including zero-width wrapper insertions at the call's two boundaries.
        rebuilt, cursor = [], 0
        for replacement in result.replacements:
            rebuilt.extend((code[cursor:replacement.original.start_utf16], replacement.generated_text))
            cursor = replacement.original.end_utf16
        rebuilt.append(code[cursor:])
        self.assertEqual(''.join(rebuilt), result.generated_text)
        for fmt in ["'%d'", 'char("%d")']:
            code = f'r=sprintf({fmt},2);'
            result = adapt_source(code, None, UNIT_PROFILE)
            self.assertNotIn('(@string)(sprintf', result.generated_text)

    def test_selection_cannot_inherit_excluded_call_type_or_wrapper(self):
        for code in ['r=sum("all");', 'r=sprintf("%d",2);']:
            a, b = code.index('"'), code.rindex('"') + 1
            result = adapt_source(code, (a, b), UNIT_PROFILE)
            self.assertEqual(result.status, 'fallback')
            self.assertEqual(result.generated_text, code[a:b])

    def test_continued_arguments_keep_physical_lines_and_dispatch(self):
        code = 'r=sum(a, ... flag\r\n "all"); s=split("a,b", ... delim\r\n ",");'
        result = self.adapted(code)
        self.assertIn("'all'", result.generated_text)
        self.assertIn("(@string)(',')", result.generated_text)
        self.assertEqual(result.generated_text.count('\r\n'), code.count('\r\n'))


@unittest.skipUnless(shutil.which('octave-cli'), 'octave-cli unavailable')
class SourceAdapterOctaveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.path = compat.root/'.packages/datatypes-1.5.0/string.m'
        cls.support = verify_package(cls.path, executable=shutil.which('octave-cli'), setup=compat.setup(), cwd=compat.root)
        if not cls.support.verified: raise AssertionError(cls.support.reason)
        cls.profile = AdapterProfile(True, package=cls.support)

    def run_code(self, code):
        result = adapt_source(code, None, self.profile)
        self.assertEqual(result.status, 'adapted', result.diagnostics)
        return fark.run([{'code': result.generated_text}], 'octave', 30)[0]

    def test_verified_package_fingerprint(self):
        self.assertTrue(self.support.verified)
        self.assertIn(hashlib.sha256(self.path.read_bytes()).hexdigest(), self.support.fingerprint)

    def test_shadowed_workspace_string_and_char_cannot_redirect_constructor(self):
        self.assertEqual(self.run_code('string=42; char=43; r="ab";'), 'string|1x1|{char|1x2|u97,98,;}')
        self.assertEqual(self.run_code('string=42; char=43; r="";'), 'string|1x1|{char|0x0|u,;}')

    def test_runtime_literal_semantics_no_backslash_decoding(self):
        self.assertEqual(self.run_code('r="a\\n";'), 'string|1x1|{char|1x3|u97,92,110,;}')
        self.assertEqual(self.run_code('r="a\\";'), 'string|1x1|{char|1x2|u97,92,;}')
        self.assertEqual(self.run_code('r="a\\\\";'), 'string|1x1|{char|1x3|u97,92,92,;}')
        self.assertEqual(self.run_code('r="a""b\'c";'), 'string|1x1|{char|1x5|u97,34,98,39,99,;}')

    def test_array_cell_plus_and_empty_scalar_shape(self):
        self.assertEqual(self.run_code('r=["a" "bb"];'), 'string|1x2|{char|1x1|u97,;char|1x2|u98,98,;}')
        self.assertEqual(self.run_code('r={"a"};'), 'cell|1x1|{string|1x1|{char|1x1|u97,;};}')
        self.assertEqual(self.run_code('r="a"+"b";'), 'string|1x1|{char|1x2|u97,98,;}')
        self.assertEqual(self.run_code('s=""; r={size(s),numel(s),char(s)};'), 'cell|1x3|{double|1x2|1,1,;double|1x1|1,;char|0x0|u,;}')
        self.assertEqual(adapt_source('s=""; r=(@char)(s);', None, self.profile).status, 'fallback')

    def test_every_string_aware_entry_and_sprintf_has_recorded_evidence(self):
        originals = {p['id']: p for p in fark.load(fark.BASE/'yoklamalar')}
        ids = sorted({i for evidence in STRING_AWARE.values() for i in evidence} | set(SPRINTF_EVIDENCE))
        probes = [originals[i] for i in ids]
        matlab = fark.recorded(probes)
        # Calibrate string identity and scalar char conversion using the
        # recorded constructor/converter inputs, preserving the input payload.
        runtime = [dict(p) for p in probes]
        for i, p in enumerate(runtime):
            if p['id'] == 'yd-string-char-empty': p['code'] = 'r=string("");'
            if p['id'] == 'yd-cast-char-row': p['code'] = 'r=char("abc");'
            if p['id'] in STRING_AWARE['size']:
                # The expected dimensions come from the stored MATLAB shape,
                # not a hand-written claim about Octave/MATLAB string sizes.
                dimensions = matlab[i].split('|')[1].split('x')
                p['code'] = p['code'].rstrip(';') + '; r=size(r);'
                matlab[i] = 'double|1x' + str(len(dimensions)) + '|' + ','.join(dimensions) + ','
                if p['id'] == 'yd-string-strings-size': p['code'] = 'r=size(["a" "b" "c"; "d" "e" "f"]);'
        generated, adaptations = fark.adapt_probes(runtime, self.profile)
        values = fark.run(generated, 'octave', 60)
        for p, m, value, adaptation in zip(probes, matlab, values, adaptations):
            with self.subTest(probe=p['id']):
                self.assertEqual(adaptation.status, 'adapted', adaptation.diagnostics)
                self.assertIn(fark.kind(m, value), fark.MATCH_KINDS, (m, value))

    def test_actual_pristine_package_is_refused_without_loading(self):
        pristine = self.path.parent/'.indymat-patches/pristine/string.m'
        self.assertTrue(pristine.is_file())
        support = verify_package(pristine, executable=shutil.which('octave-cli'), setup=compat.setup())
        self.assertFalse(support.verified)
        self.assertIn('unpatched', support.reason)
        self.assertEqual(adapt_source('r="x";', None, AdapterProfile(True, package=support)).status, 'fallback')

    def test_user_function_file_shadow_is_rejected_by_gate(self):
        with tempfile.TemporaryDirectory() as temp:
            Path(temp, 'string.m').write_text('function r=string(x)\nr=123;\nend\n')
            support = verify_package(self.path, executable=shutil.which('octave-cli'), setup=compat.setup(), cwd=temp)
            self.assertFalse(support.verified)
            result = adapt_source('r="x";', None, AdapterProfile(True, package=support))
            self.assertEqual(result.status, 'fallback')

    def test_runtime_column_calibration_unicode_and_tabs(self):
        prefixes = ["a='abc'; ", "a='ş'; ", "a='😀'; ", "\t"]
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            for i, prefix in enumerate(prefixes):
                (folder/f'column_{i}.m').write_text(f"function column_{i}()\n{prefix}for x=(@string)('a'), end\nend\n")
            code = compat.setup()
            for i in range(len(prefixes)):
                code += f"try, column_{i}(); catch e, fprintf('@@C %d %d\\n', e.stack(1).line, e.stack(1).column); disp(e.message); end\n"
            (folder/'driver.m').write_text(code)
            output = compat.octave(folder/'driver.m', folder, 30)
        columns = [(int(a), int(b)) for a, b in re.findall(r'@@C (\d+) (\d+)', output)]
        self.assertEqual(columns, [(2, len(prefix.encode('utf-8')) + 1) for prefix in prefixes])
        self.assertEqual([int(c) for c in re.findall(r'near line 2, column (\d+)', output)], [c for _, c in columns])

    def test_supported_bytes_but_unloaded_package_are_rejected(self):
        support = verify_package(self.path, executable=shutil.which('octave-cli'), setup='', cwd='/private/tmp')
        self.assertFalse(support.verified)


if __name__ == '__main__': unittest.main()
