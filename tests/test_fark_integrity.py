"""Pure Python coverage/accounting and reference identity tests; no engine calls."""
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from scripts import fark
from backend.source_adapter import AdapterProfile, PackageSupport


class MeasurementIntegrityTests(unittest.TestCase):
    def test_equal_partial_summaries_never_match(self):
        for value in ('double|1x10001|1,<partial:budget>',
                      'cell|1x41|{double|1x1|1,;}<partial:budget>',
                      'struct|1x2|<partial:structure-array>',
                      'table|2x2|<partial:unsupported-object>',
                      'cell|1x1|{cell|1x1|<partial:depth>;}',
                      'double|1x100|1,...', 'datetime|1x1|<nesne>',
                      'struct|1x2|{a;b;}'):
            with self.subTest(value=value):
                self.assertEqual(fark.kind(value, value), fark.PARTIAL_KIND)
                self.assertFalse(fark.coverage(value)['complete'])

    def test_equal_boundaries_can_prove_sampled_difference(self):
        left = 'double|1x10001|1,<partial:budget>'
        self.assertEqual(fark.kind(left, left.replace('|1,', '|2,')), 'değer farklı')
        self.assertEqual(fark.kind(left, 'double|1x10001|1,<partial:depth>'), fark.PARTIAL_KIND)
        self.assertEqual(fark.kind(left, 'double|1x10001|1,'), fark.PARTIAL_KIND)
        self.assertEqual(fark.kind(left, 'double|1x10002|1,<partial:budget>'), 'boyut farklı')

    def test_all_values_after_sixty_and_exact_integer_tokens_compare(self):
        left = 'double|1x100|' + '1,' * 100
        right = 'double|1x100|' + '1,' * 99 + '2,'
        self.assertEqual(fark.kind(left, right), 'değer farklı')
        for a, b in [('i18446744073709551615,', 'i18446744073709551614,'),
                     ('i-9223372036854775808,', 'i-9223372036854775807,'),
                     ('u1114110,', 'u1114111,')]:
            self.assertFalse(fark.close('uint64|1x1|' + a, 'uint64|1x1|' + b))
        self.assertFalse(fark.close('double|1000000000x1|1,', 'double|1000000001x1|1,'))
        self.assertTrue(fark.close('uint64|1x1|i18446744073709551615,',
                                   'uint64|1x1|i18446744073709551615,'))
        self.assertFalse(fark.close('uint64|1x1|18446744073709551615,',
                                    'uint64|1x1|18446744073709551614,'))
        self.assertFalse(fark.close('cell|1x1|{int64|1x1|-9223372036854775808,;}',
                                    'cell|1x1|{int64|1x1|-9223372036854775807,;}'))
        self.assertFalse(fark.close('logical|1x400|' + '1' * 400,
                                    'logical|1x400|' + '1' * 399 + '0'))

    def test_accounting_excludes_uncertainty_from_gains_and_differences(self):
        probes = [{'id': str(i), 'code': 'r="a";'} for i in range(6)]
        profile = AdapterProfile(True, package=PackageSupport(True, '/test/string.m', 'test', ''))
        _, adaptations = fark.adapt_probes(probes, profile)
        full = 'double|1x1|1,'
        partial = 'double|1x1|<partial:budget>'
        m = [full, full, full, partial, full, full]
        raw = [partial, 'CALISMADI', 'double|1x1|2,', partial, full, full]
        adapted = [full, full, full, partial, 'CALISMADI', 'double|1x1|2,']
        result = fark.adapted_measurement(probes, m, raw, adapted, adaptations)
        self.assertEqual((result['gained'], result['remaining'], result['partial'],
                          result['unmeasured'], result['regressions'], result['coverage_losses']),
                         (1, 1, 1, 1, 1, 1))
        self.assertEqual(result['probes'], sum(result[k] for k in
                         ('successful_matches', 'expected_error_matches', 'remaining', 'partial', 'unmeasured')))
        self.assertEqual(result['rows'][0]['coverage']['octave_raw']['status'], 'partial')
        self.assertEqual(len(result['rows'][0]['source_identity']), 64)
        self.assertEqual(result['serializer_schema'], fark.SERIALIZER_SCHEMA)

    def test_report_exposes_partial_coverage(self):
        row = {'id': 'a', 'code': 'r=1;', 'matlab': 'table|1x1|<nesne>',
               'octave': 'table|1x1|<nesne>', 'kind': fark.PARTIAL_KIND}
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp, 'report.md')
            fark.report([row], path)
            report = path.read_text()
        self.assertIn('kısmi ölçüm 1', report)
        self.assertIn('legacy-object', report)
        self.assertIn('Kapsam', report)


class ReferenceIntegrityTests(unittest.TestCase):
    def test_fixtures_exact_source_schema_and_paths_bind_cache(self):
        probes = [{'id': 'one', 'code': 'r=1;'}]
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp); fixtures = base/'yardimcilar'; fixtures.mkdir()
            (fixtures/'nested').mkdir()
            (fixtures/'a.m').write_text('a')
            helper = fixtures/'nested/b.m'; helper.write_text('b')
            cache = base/'cache.json'
            with patch.object(fark, 'BASE', base), patch.object(fark, 'RECORD', cache):
                data = fark.reference_data(probes, ['double|1x1|1,'])
                cache.write_text(json.dumps(data))
                self.assertEqual(fark.recorded(probes), ['double|1x1|1,'])
                self.assertEqual(fark.recorded([{'id': 'one', 'code': 'r=1; '}]), ['CALISMADI'])
                self.assertEqual(fark.recorded([{'id': 'two', 'code': 'r=1;'}]), ['CALISMADI'])
                helper.write_text('changed')
                self.assertEqual(fark.recorded(probes), ['CALISMADI'])
                helper.write_text('b'); helper.rename(fixtures/'nested/c.m')
                self.assertEqual(fark.recorded(probes), ['CALISMADI'])
                (fixtures/'nested/c.m').rename(helper)
                self.assertEqual(fark.recorded(probes), ['double|1x1|1,'])
                for key in ('schema', 'serializer_schema', 'reference_fixture_fingerprint'):
                    cache.write_text(json.dumps({**data, key: 'stale'}))
                    self.assertEqual(fark.recorded(probes), ['CALISMADI'])
                cache.write_text(json.dumps({'one': {'hash': fark.code_hash(probes[0]), 'matlab': 'double|1x1|1,'}}))
                self.assertEqual(fark.recorded(probes), ['CALISMADI'])

    def test_fixture_hash_is_stable_across_locations_and_creation_order(self):
        with tempfile.TemporaryDirectory() as temp:
            first, second = Path(temp, 'first'), Path(temp, 'second')
            first.mkdir(); second.mkdir()
            for name, value in [('b.m', 'b'), ('a.m', 'a')]: (first/name).write_text(value)
            for name, value in [('a.m', 'a'), ('b.m', 'b')]: (second/name).write_text(value)
            probe = {'id': 'one', 'code': 'r=1;'}
            a, b = fark.reference_context(first), fark.reference_context(second)
            self.assertEqual(a, b)
            self.assertEqual(fark.source_identity(probe, a), fark.source_identity(probe, b))
            (second/'extra.txt').write_text('also bound')
            self.assertNotEqual(a, fark.reference_context(second))


if __name__ == '__main__': unittest.main()
