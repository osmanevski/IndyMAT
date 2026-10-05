"""The differential probe tool: loading, classification and one real Octave round trip (no MATLAB needed)."""
import sys, tempfile, unittest, json
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts import fark
from backend.source_adapter import AdapterProfile, PackageSupport, adapt_source


class FarkTests(unittest.TestCase):
    def test_probe_files_load_with_unique_ids(self):
        probes = fark.load(fark.BASE / 'yoklamalar')
        self.assertGreater(len(probes), 1000)
        self.assertEqual(len({probe['id'] for probe in probes}), len(probes))
        self.assertTrue(all(probe['code'] for probe in probes))

    def test_bad_lines_and_duplicates_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            Path(temp, 'a.txt').write_text('# grup\nbir | r = 1;\nbir | r = 2;\n', encoding='utf-8')
            with self.assertRaises(ValueError): fark.load(temp)
            Path(temp, 'a.txt').write_text('kimliksiz satir\n', encoding='utf-8')
            with self.assertRaises(ValueError): fark.load(temp)

    def test_classification(self):
        self.assertEqual(fark.kind('double|1x1|1,', 'double|1x1|1,'), 'aynı')
        self.assertEqual(fark.kind('double|1x1|1,', 'double|1x1|2,'), 'değer farklı')
        self.assertEqual(fark.kind('double|1x2|1,2,', 'double|2x1|1,2,'), 'boyut farklı')
        self.assertEqual(fark.kind('string|1x1|{}', 'char|1x2|u97,98,'), 'sınıf farklı')
        self.assertEqual(fark.kind('double|1x1|1,', 'HATA|x|y'), 'Octave hata verir')
        self.assertEqual(fark.kind('HATA|x|y', 'double|1x1|1,'), 'yalnız MATLAB hata verir')
        self.assertEqual(fark.kind('HATA|a|b', 'HATA|c|d'), 'ikisi de hata')
        self.assertEqual(fark.kind('CALISMADI', 'double|1x1|1,'), 'ölçülemedi')
        # Rounding noise and the sign of zero are not differences; text and real numeric gaps are.
        self.assertEqual(fark.kind('double|1x3|1,1.54948007433e-15,1,', 'double|1x3|1,3.47554781455e-16,1,'), 'aynı')
        self.assertEqual(fark.kind('double|1x1|-0,', 'double|1x1|0,'), 'aynı')
        self.assertEqual(fark.kind('double|1x2|4+0i,2-1i,', 'double|1x2|4-0i,2-1i,'), 'aynı')
        self.assertEqual(fark.kind('double|1x1|NaN,', 'double|1x1|NaN,'), 'aynı')
        self.assertEqual(fark.kind('double|1x1|NaN,', 'double|1x1|0.6,'), 'değer farklı')
        self.assertEqual(fark.kind('double|1x2|0.01008727,1,', 'double|1x2|0.01018839,1,'), 'değer farklı')
        self.assertEqual(fark.kind('char|1x2|u97,98,', 'char|1x2|u97,99,'), 'değer farklı')
        self.assertEqual(fark.kind('char|1x1|u97,', 'char|1x1|u97.0000000001,'), 'değer farklı')

    def test_recorded_matlab_results_cover_the_probes(self):
        probes = fark.load(fark.BASE / 'yoklamalar')
        values = fark.recorded(probes)
        self.assertEqual(len(values), len(probes))
        self.assertLess(sum(value == 'CALISMADI' for value in values), 20, 'rerun scripts/fark.py with MATLAB after editing probes')
        changed = dict(probes[0], code=probes[0]['code'] + ' ')
        self.assertEqual(fark.recorded([changed]), ['CALISMADI'])

    def test_octave_round_trip(self):
        probes = [{'id': 'a', 'code': 'r = int8(100) + int8(100);'}, {'id': 'b', 'code': "r = {'x', [1 2; 3 4]};"},
                  {'id': 'c', 'code': 'x = 1:3; r = x(5);'}, {'id': 'd', 'code': 'r = )'}]
        values = fark.run(probes, 'octave', timeout=120)
        self.assertEqual(values[0], 'int8|1x1|127,')
        self.assertEqual(values[1], 'cell|1x2|{char|1x1|u120,;double|2x2|1,3,2,4,;}')
        self.assertTrue(values[2].startswith('HATA|'))
        self.assertTrue(values[3].startswith('HATA|'), values[3])


class FarkAdaptedTests(unittest.TestCase):
    profile = AdapterProfile(True, package=PackageSupport(True, '/test/string.m', 'test-fingerprint', ''))

    def test_adapts_bodies_only_runner_remains_native(self):
        probes = [{'id': 'a', 'code': 'r="x";', 'group': 'g'}]
        generated, adaptations = fark.adapt_probes(probes, self.profile)
        self.assertEqual(probes[0]['code'], 'r="x";')
        self.assertEqual(generated[0]['code'], "r=(@string)('x');")
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            fark.runner(generated, folder)
            self.assertIn("r=(@string)('x');", (folder/'uy_yoklama_0000.m').read_text())
            driver = (folder/'uy_yoklama_surucu.m').read_text()
            self.assertNotIn('(@string)', driver)
            self.assertIn("sprintf('\\n')", driver)
        self.assertEqual(adaptations[0].package_fingerprint, 'test-fingerprint')

    def test_gain_regression_expected_error_and_fallback_counts(self):
        probes = [{'id': str(i), 'code': code} for i, code in enumerate(['r="x";', 'r="x";', 'r="x";', 'disp "x"'])]
        _, adaptations = fark.adapt_probes(probes, self.profile)
        matlab = ['string|1x1|x', 'double|1x1|1,', 'HATA|m|bad', 'double|1x1|1,']
        raw = ['char|1x1|x', 'double|1x1|1,', 'double|1x1|1,', 'double|1x1|1,']
        adapted = ['string|1x1|x', 'HATA|o|object', 'HATA|o|different error', 'double|1x1|1,']
        result = fark.adapted_measurement(probes, matlab, raw, adapted, adaptations)
        self.assertEqual((result['gained'], result['remaining'], result['regressions'], result['fallbacks']), (2, 1, 1, 1))
        self.assertEqual((result['gained_successful'], result['gained_expected_error']), (1, 1))
        self.assertEqual(result['regression_ids'], ['1'])
        row = result['rows'][0]
        self.assertEqual(row['original_hash'], fark.code_hash(probes[0]))
        self.assertNotEqual(row['generated_hash'], row['original_hash'])
        self.assertEqual(row['octave_raw'], raw[0])
        self.assertEqual(row['octave_adapted'], adapted[0])

    def test_unverified_package_fallback_is_reported(self):
        probes = [{'id': 'a', 'code': 'r="x";'}]
        generated, adaptations = fark.adapt_probes(probes, AdapterProfile(True))
        self.assertEqual(generated, probes)
        self.assertEqual(adaptations[0].status, 'fallback')
        self.assertEqual(adaptations[0].diagnostics[0].code, 'package-unverified')

    def test_second_run_noise_in_fallback_is_not_a_gain_or_regression(self):
        probes = [{'id': 'noise', 'code': 'r=unknown("x");'}]
        _, adaptations = fark.adapt_probes(probes, self.profile)
        self.assertEqual(adaptations[0].status, 'fallback')
        for raw, adapted in [('double|1x1|1,', 'double|1x1|2,'), ('double|1x1|2,', 'double|1x1|1,')]:
            result = fark.adapted_measurement(probes, ['double|1x1|1,'], [raw], [adapted], adaptations)
            self.assertEqual((result['gained'], result['regressions'], result['fallbacks']), (0, 0, 1))
            self.assertEqual(result['rows'][0]['octave_adapted'], adapted)

    def test_adapted_mode_does_not_write_raw_files_or_matlab_reference(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base/'yoklamalar').mkdir()
            (base/'yoklamalar'/'a.txt').write_text('a | r="x";\n')
            raw_files = {name: b'preserve exact bytes' for name in ('farklar.json', 'yoklama-matlab.json')}
            for name, data in raw_files.items(): (base/name).write_bytes(data)
            with patch.object(fark, 'BASE', base), patch.object(fark, 'recorded', return_value=['string|1x1|x']) as recorded, \
                 patch.object(fark, 'run', side_effect=[['char|1x1|x'], ['string|1x1|x']]) as run, \
                 patch.object(fark, 'adapted_profile', return_value=self.profile), patch.object(fark, 'report') as report, \
                 patch.object(sys, 'argv', ['fark.py', '--octave-only', '--adapted']), patch('builtins.print'):
                self.assertEqual(fark.main(), 0)
            self.assertEqual(run.call_count, 2)
            recorded.assert_called_once()
            report.assert_not_called()
            for name, data in raw_files.items(): self.assertEqual((base/name).read_bytes(), data)
            data = json.loads((base/'farklar-uyarlanmis.json').read_text())
            self.assertEqual(data['gained'], 1)
            self.assertEqual(data['rows'][0]['adaptation']['package_fingerprint'], 'test-fingerprint')

    def test_explicit_adapted_output_keeps_repository_reports_untouched(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base/'yoklamalar').mkdir()
            (base/'yoklamalar'/'a.txt').write_text('a | r="x";\n')
            default = base/'farklar-uyarlanmis.json'
            default.write_bytes(b'preserve baseline')
            output = base/'external.json'
            with patch.object(fark, 'BASE', base), patch.object(fark, 'recorded', return_value=['string|1x1|x']), \
                 patch.object(fark, 'run', side_effect=[['char|1x1|x'], ['string|1x1|x']]), \
                 patch.object(fark, 'adapted_profile', return_value=self.profile), \
                 patch.object(sys, 'argv', ['fark.py', '--octave-only', '--adapted', '--output', str(output)]), patch('builtins.print'):
                self.assertEqual(fark.main(), 0)
            self.assertEqual(default.read_bytes(), b'preserve baseline')
            self.assertEqual(json.loads(output.read_text())['regressions'], 0)

    def test_adapted_matlab_run_receives_original_bodies(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base/'yoklamalar').mkdir()
            (base/'yoklamalar'/'a.txt').write_text('a | r="x";\n')
            with patch.object(fark, 'BASE', base), patch.object(fark, 'run', return_value=['string|1x1|x']) as run, \
                 patch.object(fark, 'adapted_profile', return_value=self.profile), \
                 patch.object(sys, 'argv', ['fark.py', '--adapted']), patch('builtins.print'):
                self.assertEqual(fark.main(), 0)
            self.assertEqual(run.call_args_list[0].args[1], 'matlab')
            self.assertEqual(run.call_args_list[0].args[0][0]['code'], 'r="x";')
            self.assertEqual(run.call_args_list[2].args[0][0]['code'], "r=(@string)('x');")
            self.assertFalse((base/'yoklama-matlab.json').exists())

    def test_print_lists_each_regression_and_fallback(self):
        probes = [{'id': 'reg', 'code': 'r="x";'}, {'id': 'fallback', 'code': 'disp "x"'}]
        _, adaptations = fark.adapt_probes(probes, self.profile)
        measurement = fark.adapted_measurement(probes, ['double|1x1|1,'] * 2, ['double|1x1|1,'] * 2,
                                              ['HATA|o|unsupported string', 'double|1x1|1,'], adaptations)
        with patch('builtins.print') as printed:
            fark.print_adapted(measurement)
        output = '\n'.join(call.args[0] for call in printed.call_args_list)
        self.assertIn('REGRESSION reg:', output)
        self.assertIn('unsupported string', output)
        self.assertIn('FALLBACK fallback:', output)
        self.assertIn('Quoted command-form', output)


if __name__ == '__main__':
    unittest.main()
