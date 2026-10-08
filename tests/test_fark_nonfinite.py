"""Prevent false compatibility matches for infinite numerical results."""
import unittest
from scripts.fark import close, kind

class NonfiniteComparisonTests(unittest.TestCase):
    def test_infinity_never_matches_finite_values_or_opposite_sign(self):
        for a,b in [('Inf','2'), ('-Inf','-2'), ('Inf','-Inf'), ('Inf','1e308'), ('-Inf','0')]:
            for left,right in [(a,b),(b,a)]:
                with self.subTest(left=left,right=right):
                    self.assertFalse(close('double|1x1|'+left, 'double|1x1|'+right))
                    self.assertEqual(kind('double|1x1|'+left, 'double|1x1|'+right), 'değer farklı')
    def test_equal_nonfinite_and_finite_roundoff_still_match(self):
        for a,b in [('Inf','+Inf'),('-Inf','-Inf'),('NaN','NaN'),('0','-0'),('1.00000000001','1')]:
            self.assertTrue(close('double|1x1|'+a,'double|1x1|'+b),(a,b))
        for a,b in [('NaN','Inf'),('NaN','0'),('Inf','NaN')]:
            self.assertFalse(close('double|1x1|'+a,'double|1x1|'+b),(a,b))
    def test_nested_payload_and_measured_norm_gap(self):
        self.assertFalse(close('cell|1x1|{double|1x2|1,Inf,;}', 'cell|1x1|{double|1x2|1,2,;}'))
        self.assertEqual(kind('double|1x1|Inf,','double|1x1|2,'),'değer farklı')

if __name__=='__main__':unittest.main()
