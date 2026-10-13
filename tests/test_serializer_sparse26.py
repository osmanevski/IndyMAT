"""Serializer acceptance through octave-cli only; never Qt or MATLAB."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from scripts import fark

ROOT = Path(__file__).resolve().parents[1]
SCRATCH = Path.home()/'.cache/indymat-fleet/dalga26/uyum-tmp'


class SerializerSparseTests(unittest.TestCase):
    def run_octave(self, code, cwd=ROOT):
        exe = shutil.which('octave-cli')
        if not exe:
            self.skipTest('octave-cli unavailable')
        result = subprocess.run([exe, '--quiet', '--no-gui', '--no-init-file',
                                 '--no-site-file', '--no-history', '--eval', code],
                                cwd=cwd, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        return result.stdout

    def test_sparse_values_and_storage_are_serialized_without_full_allocation(self):
        code = f"addpath('{ROOT}/uyumluluk/yardimcilar');" + r"""
assert(strcmp(uy_ozet(sparse([1 0;0 2])), 'double|2x2|<sparse>1,0,0,2,'));
assert(strcmp(uy_ozet(sparse(logical([1 0;0 1]))), 'logical|2x2|<sparse>1001'));
assert(strcmp(uy_ozet(sparse(1+2i)), 'double|1x1|<sparse>1+2i,'));
assert(strcmp(uy_ozet(sparse(0,3)), 'double|0x3|<sparse>'));
assert(~strcmp(uy_ozet(sparse(eye(2))),uy_ozet(eye(2))));
s=uy_ozet({sparse([NaN Inf])}); assert(~isempty(strfind(s,'<sparse>NaN,Inf,')));
s=uy_ozet(sparse(100000,100000)); assert(~isempty(strfind(s,'<partial:budget>')));
"""
        self.run_octave(code)

    def test_all_complete_old_nonsparse_families_are_byte_identical(self):
        code = f"addpath('{ROOT}/uyumluluk/yardimcilar');addpath('{ROOT}/tests/fixtures');" + r"""
values={[],zeros(0,3),[1 2;3 4],single([1 -2]),[Inf -Inf NaN],1+2i, ...
 int64([-2 0 3]),intmin('int64'),intmax('uint64'),logical([1 0;0 1]), ...
 '',char([0 32 65 255]),{'x',[1 2],{true}},struct(),struct('a',1,'b',{{2,'x'}}), ...
 ones(1,10001),num2cell(ones(1,10001)),@sin};
for k=1:numel(values), assert(strcmp(uy_ozet(values{k}),uy_ozet_v2_reference(values{k}))); end
"""
        self.run_octave(code)

    def test_struct_arrays_include_fields_order_values_empty_and_budget(self):
        code=f"addpath('{ROOT}/uyumluluk/yardimcilar');"+r"""
s=uy_ozet(struct('a',{1,2},'b',{3,4})); assert(isempty(strfind(s,'<partial:')));
assert(~strcmp(s,uy_ozet(struct('a',{1,9},'b',{3,4}))));
assert(strcmp(s,uy_ozet(struct('b',{3,4},'a',{1,2}))));
assert(~strcmp(uy_ozet(struct('a',{})),uy_ozet(struct('b',{}))));
assert(~strcmp(uy_ozet(struct('a',{})),uy_ozet(repmat(struct('a',1),0,2))));
s=uy_ozet(repmat(struct('a',1),1,10001)); assert(~isempty(strfind(s,'<partial:budget>')));
"""
        self.run_octave(code)
        full='struct|1x2|fields=cell|1x1|{char|1x1|u97,;};elements={struct|1x1|{u97,=double|1x1|1,;};struct|1x1|{u97,=double|1x1|2,;};}'
        self.assertEqual(fark.kind(full,full),'aynı')
        self.assertEqual(fark.kind('struct|1x2|{a;b;}','struct|1x2|{a;b;}'),fark.PARTIAL_KIND)

    def test_serializer_failure_cannot_become_expected_probe_rejection(self):
        SCRATCH.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=SCRATCH) as tmp:
            folder=Path(tmp)
            fark.runner([{'code':'r=sparse(1);'},
                         {'code':"error('MATLAB:printf:invalidInputAttribute','probe failure');"}],folder)
            (folder/'uy_ozet.m').write_text("function s=uy_ozet(x)\nerror('MATLAB:printf:invalidInputAttribute','Function is not defined for sparse inputs.');\nend\n")
            stdout=self.run_octave('uy_yoklama_surucu();',folder)
            values,complete=fark.parse(stdout,2)
            self.assertTrue(complete)
            self.assertTrue(values[0].startswith('SERIALIZER_ERROR|MATLAB:printf:'))
            self.assertTrue(values[1].startswith('HATA|MATLAB:printf:'))
            self.assertEqual(fark.kind(values[0],values[0]),'ölçülemedi')
            self.assertEqual(fark.kind(values[0],values[1]),'ölçülemedi')
            self.assertEqual(fark.kind(values[1],values[1]),'ikisi de hata')
            self.assertFalse(fark.coverage(values[0])['complete'])


if __name__ == '__main__':
    unittest.main()
