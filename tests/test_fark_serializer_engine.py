"""Real MATLAB/Octave serializer acceptance. Root runs this sequentially.

Run: python3 -B -m unittest tests.test_fark_serializer_engine
Uses isolated helper processes, no result/reference JSON writes.
"""
import tempfile
import unittest
from pathlib import Path
from scripts import compat


class SerializerEngineTests(unittest.TestCase):
    def test_shared_serializer_contract_in_both_engines(self):
        code = r"""
% Full tails beyond the old 60-element and 40-cell limits.
x=ones(1,200); y=x; y(200)=2;
assert(~strcmp(uy_ozet(x),uy_ozet(y)));
assert(isempty(strfind(uy_ozet(x),'<partial:')));
c=num2cell(ones(1,100)); d=c; d{100}=2;
assert(~strcmp(uy_ozet(c),uy_ozet(d)));
assert(isempty(strfind(uy_ozet(c),'<partial:')));
% Bound the whole result, including many small nested containers.
s=uy_ozet(ones(1,10001)); assert(~isempty(strfind(s,'<partial:budget>')));
s=uy_ozet(num2cell(ones(1,10001))); assert(~isempty(strfind(s,'<partial:budget>')));
x=1; for k=1:10, x={x}; end
assert(~isempty(strfind(uy_ozet(x),'<partial:depth>')));
s=uy_ozet(struct('a',{1,2})); assert(~isempty(strfind(s,'<partial:structure-array>')));
s=uy_ozet(@sin); assert(~isempty(strfind(s,'<partial:unsupported-object>')));
s=uy_ozet(table([1;2])); assert(~isempty(strfind(s,'<partial:unsupported-object>')));
% No uint64/int64 -> double conversion, especially adjacent large integers.
u=intmax('uint64'); v=u-uint64(1);
assert(strcmp(uy_ozet(u),'uint64|1x1|18446744073709551615,'));
assert(strcmp(uy_ozet(v),'uint64|1x1|18446744073709551614,'));
assert(strcmp(uy_ozet(intmin('int64')),'int64|1x1|-9223372036854775808,'));
assert(strcmp(uy_ozet(intmax('int64')),'int64|1x1|9223372036854775807,'));
assert(strcmp(uy_ozet(int8(-12)),'int8|1x1|-12,'));
assert(strcmp(uy_ozet(uint8(0)),'uint8|1x1|0,'));
assert(strcmp(uy_ozet([Inf -Inf NaN]),'double|1x3|Inf,-Inf,NaN,'));
% Stable column-major serialization and sorted scalar-struct fields.
assert(strcmp(uy_ozet([1 2;3 4]),'double|2x2|1,3,2,4,'));
assert(strcmp(uy_ozet([]),'double|0x0|'));
assert(strcmp(uy_ozet({}),'cell|0x0|{}'));
assert(strcmp(uy_ozet(1+2i),'double|1x1|1+2i,'));
a=struct('b',2,'a',1); b=struct('a',1,'b',2);
assert(strcmp(uy_ozet(a),uy_ozet(b)));
assert(isempty(strfind(uy_ozet(a),'<partial:')));
% User field names resembling markers cannot fabricate partial status.
a=struct(); a.('marker')='<partial:budget>';
assert(isempty(strfind(uy_ozet(a),'<partial:')));
"""
        cases = [{'id': 'serializer-schema-two', 'code': code.splitlines()}]
        for engine in ('octave', 'matlab'):
            with self.subTest(engine=engine), tempfile.TemporaryDirectory(prefix='serializer-acceptance-') as temp:
                results, version, _ = compat.execute(cases, Path(temp), compat.BASE/'yardimcilar', 120, engine)
                self.assertTrue(version, f'{engine}: process did not finish successfully')
                self.assertEqual(results[cases[0]['id']]['status'], 'OK', results[cases[0]['id']]['message'])


if __name__ == '__main__': unittest.main()
