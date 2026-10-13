"""Sparse logical payloads must never receive floating-point tolerance."""
import unittest
from scripts import fark

class SparseLogical27Tests(unittest.TestCase):
    def test_unequal_sparse_logical_payloads(self):
        for prefix in ('', 'cell|1x1|{'):
            a=prefix+'logical|1x10|<sparse>1000000000'
            b=prefix+'logical|1x10|<sparse>1000000001'
            self.assertFalse(fark.close(a,b))
            self.assertFalse(fark.close(b,a))
            self.assertTrue(fark.close(a,a))

    def test_exact_dense_and_sparse_and_float_tolerance(self):
        self.assertFalse(fark.close('logical|1x10|1000000000','logical|1x10|1000000001'))
        self.assertFalse(fark.close('logical|1x1|<sparse>1','logical|1x1|1'))
        self.assertTrue(fark.close('double|1x1|<sparse>1,','double|1x1|<sparse>1.0000000001,'))
