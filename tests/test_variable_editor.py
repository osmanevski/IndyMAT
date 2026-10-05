import json
import tempfile
import time
import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from backend.kernel import Kernel
from backend.workspace_actions import variable_read_request, variable_write_request, variable_scalar

ROOT = Path(__file__).resolve().parents[1]


class VariableEditorValidationTests(unittest.TestCase):
    def test_utf8_text_and_negative_zero_validation(self):
        self.assertEqual(variable_scalar('char', {'type':'text','value':'ğİş🙂'})['value'], 'ğİş🙂')
        self.assertEqual(variable_scalar('char', {'type':'text','value':''})['value'], '')
        for value in [{'type':'char','value':'z'}, {'type':'text','value':'\ud800'}, {'type':'text','value':'ğ'*5001}]:
            with self.assertRaises(ValueError):
                variable_scalar('char', value)
        for kind in ('double', 'single'):
            self.assertEqual(variable_scalar(kind, {'type':'special','value':'-0'}), {'type':'special','value':'-0'})
        with self.assertRaises(ValueError):
            variable_scalar('int8', {'type':'special','value':'-0'})

    def test_read_bounds_and_typed_paths(self):
        request = variable_read_request({'action':'read','name':'A','path':[{'kind':'cell','indices':[1,2]},{'kind':'field','name':'sonuç'}],'row':1,'column':1,'rows':100,'columns':30,'slices':[2],'epoch':1})
        self.assertEqual(request['path'][0]['indices'], [1,2])
        for bad in [
            {'action':'read','name':'A;clear','path':[],'row':1,'column':1,'rows':1,'columns':1,'slices':[],'epoch':1},
            {'action':'read','name':'A','path':[],'row':0,'column':1,'rows':1,'columns':1,'slices':[],'epoch':1},
            {'action':'read','name':'A','path':[],'row':1,'column':1,'rows':101,'columns':1,'slices':[],'epoch':1},
            {'action':'read','name':'A','path':[],'row':1,'column':1,'rows':100,'columns':31,'slices':[],'epoch':1},
        ]:
            with self.assertRaises(ValueError, msg=bad):
                variable_read_request(bad)

    def test_write_scalars_are_typed_and_bounded(self):
        base = {'action':'write','name':'A','path':[],'row':1,'column':1,'height':1,'width':1,'slices':[],'epoch':1,'read_job':'a'*32,'class':'int8','size':[1,1]}
        self.assertEqual(variable_write_request({**base,'values':[{'type':'integer','value':'-128'}]})['values'][0]['value'], '-128')
        for value in [{'type':'integer','value':'128'},{'type':'number','value':1},{'type':'integer','value':'1;system'}]:
            with self.assertRaises(ValueError, msg=value):
                variable_write_request({**base,'values':[value]})
        with self.assertRaises(ValueError):
            variable_write_request({**base,'path':[{'kind':'cell','indices':[1,1]}],'values':[{'type':'integer','value':'1'}]})


class VariableEditorKernelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        root = Path(cls.temp.name)
        cls.work = root/'work'
        cls.work.mkdir()
        cls.kernel = Kernel(ROOT, root/'runtime', cls.work)
        cls.wait()

    @classmethod
    def tearDownClass(cls):
        cls.kernel.close()
        cls.temp.cleanup()

    @classmethod
    def wait(cls, seconds=30):
        deadline = time.monotonic()+seconds
        while time.monotonic() < deadline:
            state = cls.kernel.snapshot()
            if state['status'] not in ('starting','running','stopping'):
                return state
            time.sleep(.05)
        raise AssertionError(cls.kernel.snapshot())

    def action(self, mode, request):
        self.kernel.workspace_action(mode, request)
        return self.wait()

    def code(self, source):
        self.kernel.submit(source)
        return self.wait()

    def read(self, name, path=None):
        state = self.action('variable-read', {'name':name,'path':path or [],'row':1,'column':1,'rows':100,'columns':30,'slices':[],'epoch':self.kernel.generation})
        self.assertFalse(state['error'], state)
        return state['variable_action']

    def write_one(self, name, kind, size, value):
        return self.action('variable-write', {'name':name,'path':[],'row':1,'column':1,'height':1,'width':1,'slices':[],'epoch':self.kernel.generation,'read_job':'a'*32,'class':kind,'size':size,'values':[value]})

    def test_utf8_row_round_trip_and_byte_edits_refused(self):
        self.assertFalse(self.code("ve_utf='ğ'; ve_bad=char([122 159]); ve_equal=char('ğ','İ'); ve_unequal=char('ğ','ab'); ve_large=repmat('ğ',1,10000);")['error'])
        page = self.read('ve_utf')
        self.assertEqual(page['kind'], 'char-text')
        self.assertEqual(page['text'], 'ğ')
        self.assertTrue(page['editable'])
        rejected = self.write_one('ve_utf', 'char', [1,2], {'type':'char','value':'z'})
        self.assertTrue(rejected['error'])
        self.assertFalse(self.code('assert(isequal(uint8(ve_utf),uint8([196 159])));')['error'])
        written = self.write_one('ve_utf', 'char', [1,2], {'type':'text','value':'zİş🙂'})
        self.assertFalse(written['error'], written)
        page = self.read('ve_utf')
        self.assertEqual(page['text'], 'zİş🙂')
        self.assertEqual(page['size'], [1,9])
        self.assertFalse(self.code("assert(strcmp(native2unicode(uint8(ve_utf),'UTF-8'),'zİş🙂'));")['error'])
        # Whole-text replacement can shrink to the empty string and grow again.
        self.assertFalse(self.write_one('ve_utf', 'char', [1,9], {'type':'text','value':''})['error'])
        empty = self.read('ve_utf')
        self.assertEqual(empty['text'], '')
        self.assertFalse(self.write_one('ve_utf', 'char', empty['size'], {'type':'text','value':'ğ'})['error'])
        bad = self.read('ve_bad')
        self.assertFalse(bad['editable'])
        self.assertIn('7A 9F', bad['text'])
        self.assertNotIn('\ufffd', json.dumps(bad, ensure_ascii=False))
        self.assertTrue(self.write_one('ve_bad','char',[1,2],{'type':'text','value':'z'})['error'])
        equal = self.read('ve_equal')
        self.assertEqual(equal['text'], 'ğ\nİ')
        self.assertFalse(equal['editable'])
        unequal = self.read('ve_unequal')
        self.assertEqual(unequal['text'], 'ğ\nab')
        self.assertIn('eşit değil', unequal['note'])
        self.assertFalse(unequal['editable'])
        self.assertTrue(self.write_one('ve_equal','char',[2,2],{'type':'text','value':'x'})['error'])
        large = self.read('ve_large')
        self.assertFalse(large['editable'])
        self.assertEqual(large['text'], '')
        self.assertIn('sınırı', large['note'])

    def test_negative_zero_preserves_sign_and_class(self):
        for kind in ('double', 'single'):
            self.assertFalse(self.code(f"ve_zero={kind}(1);")['error'])
            # Same JSON round trip used by the browser; negative zero is tagged.
            scalar = json.loads(json.dumps({'type':'special','value':'-0'}))
            self.assertFalse(self.write_one('ve_zero',kind,[1,1],scalar)['error'])
            self.assertEqual(self.read('ve_zero')['rows'], [['-0']])
            checked = self.code(f"assert(isa(ve_zero,'{kind}') && signbit(ve_zero) && isinf(1/ve_zero) && 1/ve_zero<0);")
            self.assertFalse(checked['error'], checked)

    def test_object_preview_never_invokes_disp(self):
        # A disp method that errors proves the bound precedes any display work.
        fixture = self.work/'VariableEditorDisplayProbe.m'
        fixture.write_text("classdef VariableEditorDisplayProbe\n methods\n function disp(obj)\n error('UNBOUNDED_DISP_CALLED');\n end\n end\nend\n")
        self.assertFalse(self.code("ve_object={VariableEditorDisplayProbe()}; ve_handle={@(x) x+1}; ve_map={containers.Map({'a'},{1})};")['error'])
        for name, expected in [('ve_object','VariableEditorDisplayProbe'),('ve_handle','Fonksiyon tutamacı'),('ve_map','Anahtar/değer haritası')]:
            page = self.read(name, [{'kind':'cell','indices':[1,1]}])
            self.assertEqual(page['kind'], 'detail')
            self.assertIn(expected, page['text'])
            self.assertLess(len(page['text']), 500)
        fixture.unlink()

    def test_large_datatypes_string_is_metadata_only(self):
        setup = self.code("pkg load datatypes; ve_strings={repmat(string('ğ'),1,100000)};")
        self.assertFalse(setup['error'], setup)
        page = self.read('ve_strings', [{'kind':'cell','indices':[1,1]}])
        self.assertEqual(page['class'], 'string')
        self.assertEqual(page['size'], [1,100000])
        self.assertIn('İçerik genişletilmedi', page['text'])
        self.assertLess(len(page['text']), 500)

    def test_nd_pages_navigation_and_exact_integer_writes(self):
        generation = self.kernel.generation
        setup = self.code("ve_keep=731; ve_nd=reshape(1:48,[4,3,4]); ve_cell={reshape(1:8,[2,2,2]),struct('field',single(7))}; ve_i=int64([0,1]); ve_z=[1+2i,3-4i];")
        self.assertFalse(setup['error'], setup)
        read = self.action('variable-read', {'name':'ve_nd','path':[],'row':2,'column':2,'rows':2,'columns':2,'slices':[3],'epoch':generation})
        self.assertFalse(read['error'], read)
        page = read['variable_action']
        self.assertEqual(page['size'], [4,3,4])
        self.assertEqual(page['rows'], [['30','34'],['31','35']])
        nested = self.action('variable-read', {'name':'ve_cell','path':[{'kind':'cell','indices':[1,1]}],'row':1,'column':1,'rows':2,'columns':2,'slices':[2],'epoch':generation})
        self.assertFalse(nested['error'], nested)
        self.assertEqual(nested['variable_action']['rows'], [['5','7'],['6','8']])
        field = self.action('variable-read', {'name':'ve_cell','path':[{'kind':'cell','indices':[1,2]},{'kind':'field','name':'field'}],'row':1,'column':1,'rows':1,'columns':1,'slices':[],'epoch':generation})
        self.assertFalse(field['error'], field)
        self.assertEqual(field['variable_action']['rows'], [['7']])
        complex_page = self.action('variable-read', {'name':'ve_z','path':[],'row':1,'column':1,'rows':1,'columns':2,'slices':[],'epoch':generation})
        self.assertEqual(complex_page['variable_action']['rows'], [['1+2i','3-4i']])
        self.assertFalse(complex_page['variable_action']['editable'])
        written = self.action('variable-write', {'name':'ve_i','path':[],'row':1,'column':1,'height':1,'width':2,'slices':[],'epoch':generation,'read_job':'a'*32,'class':'int64','size':[1,2],'values':[{'type':'integer','value':'-9223372036854775808'},{'type':'integer','value':'9223372036854775807'}]})
        self.assertFalse(written['error'], written)
        checked = self.code("assert(ve_keep==731 && ve_i(1)==intmin('int64') && ve_i(2)==intmax('int64'));")
        self.assertFalse(checked['error'], checked)
        self.assertEqual(self.kernel.generation, generation)

    def test_metadata_and_growth_fail_before_mutation(self):
        self.assertFalse(self.code("ve_stale=int8([1 2;3 4]);")['error'])
        generation = self.kernel.generation
        bad_size = self.action('variable-write', {'name':'ve_stale','path':[],'row':1,'column':1,'height':1,'width':1,'slices':[],'epoch':generation,'read_job':'b'*32,'class':'int8','size':[1,4],'values':[{'type':'integer','value':'9'}]})
        self.assertIn('boyutu değişti', bad_size['error'])
        growth = self.action('variable-write', {'name':'ve_stale','path':[],'row':2,'column':2,'height':1,'width':2,'slices':[],'epoch':generation,'read_job':'c'*32,'class':'int8','size':[2,2],'values':[{'type':'integer','value':'8'},{'type':'integer','value':'9'}]})
        self.assertIn('büyütmez', growth['error'])
        checked = self.code("assert(isequal(ve_stale,int8([1 2;3 4])));")
        self.assertFalse(checked['error'], checked)



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
    unittest.main()
