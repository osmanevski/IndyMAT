"""The differential probe tool: loading, classification and one real Octave round trip (no MATLAB needed)."""
import sys, tempfile, unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts import fark


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

    def test_octave_round_trip(self):
        probes = [{'id': 'a', 'code': 'r = int8(100) + int8(100);'}, {'id': 'b', 'code': "r = {'x', [1 2; 3 4]};"},
                  {'id': 'c', 'code': 'x = 1:3; r = x(5);'}, {'id': 'd', 'code': 'r = )'}]
        values = fark.run(probes, 'octave', timeout=120)
        self.assertEqual(values[0], 'int8|1x1|127,')
        self.assertEqual(values[1], 'cell|1x2|{char|1x1|u120,;double|2x2|1,3,2,4,;}')
        self.assertTrue(values[2].startswith('HATA|'))
        self.assertTrue(values[3].startswith('HATA|'), values[3])


if __name__ == '__main__':
    unittest.main()
