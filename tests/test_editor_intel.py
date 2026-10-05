import os
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from backend.symbols import SymbolIndex,parse_source


class EditorIntelTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name).resolve();self.index=SymbolIndex()
 def tearDown(self):self.temp.cleanup()
 def test_parser_ignores_strings_line_and_block_comments(self):
  source="""function y = gercek(x)
text = 'function fake(a)';
% function commented(x)
%{
function blocked(x)
%}
y = gercek(x);
end
"""
  functions,uses,limited=parse_source(source,self.root/'gercek.m','gercek')
  self.assertEqual([item['name'] for item in functions],['gercek'])
  self.assertEqual([(item['line'],item['column']) for item in uses],[(1,14),(7,5)])
  self.assertFalse(limited)
 def test_current_folder_only_and_symlinks_are_not_followed(self):
  (self.root/'bir.m').write_text('function y = bir(x)\ny=x;\nend\n')
  nested=self.root/'alt';nested.mkdir();(nested/'iki.m').write_text('function iki\nend\n')
  outside=Path(self.temp.name).parent/('outside-'+self.root.name+'.m');outside.write_text('function dis\nend\n')
  try:
   (self.root/'bag.m').symlink_to(outside)
   result=self.index.scan(self.root,(self.root,))
   self.assertEqual([item['name'] for item in result['functions']],['bir'])
   self.assertEqual(result['scanned_files'],1)
  finally:outside.unlink(missing_ok=True)
 def test_file_count_byte_and_occurrence_limits_are_reported(self):
  self.index.MAX_FILES=1;self.index.MAX_OCCURRENCES=2
  (self.root/'a.m').write_text('function a\na; a; a;\nend\n')
  (self.root/'b.m').write_text('function b\nend\n')
  result=self.index.scan(self.root,(self.root,),'a')
  self.assertEqual(result['scanned_files'],1);self.assertEqual(len(result['occurrences']),2);self.assertTrue(result['truncated'])
  self.index.MAX_FILE_BYTES=4
  result=self.index.scan(self.root,(self.root,))
  self.assertEqual(result['scanned_files'],0);self.assertTrue(result['truncated'])
  self.index.MAX_FILES=10;self.index.MAX_FILE_BYTES=500_000;self.index.MAX_TOTAL_BYTES=10
  result=self.index.scan(self.root,(self.root,))
  self.assertEqual(result['scanned_files'],0);self.assertTrue(result['truncated'])
  self.index.MAX_TOTAL_BYTES=2_000_000;self.index.MAX_ENTRIES=1
  result=self.index.scan(self.root,(self.root,))
  self.assertTrue(result['truncated'])
  self.index.MAX_ENTRIES=2000;self.index.TIME_LIMIT=0
  result=self.index.scan(self.root,(self.root,))
  self.assertEqual(result['scanned_files'],0);self.assertTrue(result['truncated'])
 def test_invalid_name_and_symlink_folder_are_rejected(self):
  with self.assertRaisesRegex(ValueError,'Geçerli'):self.index.scan(self.root,(self.root,),'bad-name')
  link=self.root.parent/('link-'+self.root.name);link.symlink_to(self.root,True)
  try:
   with self.assertRaises(PermissionError):self.index.scan(link,(self.root.parent,))
  finally:link.unlink()
 def test_dense_declarations_are_bounded_without_relying_on_time(self):
  # Review #7: 1.87 MB previously expanded into >7 MB of function JSON.
  source='function f\n'*170_000
  self.assertEqual(len(source.encode()),1_870_000)
  functions,uses,limited=parse_source(source,'probe.m')
  self.assertEqual(len(functions),self.index.MAX_FUNCTIONS)
  self.assertTrue(limited)
  payload={'functions':functions,'occurrences':uses,'truncated':limited}
  self.assertLessEqual(len(json.dumps(payload,ensure_ascii=False).encode()),self.index.MAX_RESULT_BYTES)
 def test_function_cap_is_shared_across_files(self):
  self.index.MAX_FUNCTIONS=50
  for name in ('a.m','b.m','c.m'):(self.root/name).write_text('function f\n'*30)
  result=self.index.scan(self.root,(self.root,))
  self.assertEqual(len(result['functions']),50)
  self.assertEqual(result['scanned_files'],2)
  self.assertTrue(result['truncated'])
 def test_result_byte_cap_counts_long_names_and_both_arrays(self):
  name='f'*1000
  (self.root/'dense.m').write_text(f'function {name}\n'*400)
  self.index.TIME_LIMIT=10  # Exercise the byte limit, independently of timing.
  result=self.index.scan(self.root,(self.root,),name)
  raw=json.dumps(result,ensure_ascii=False).encode('utf-8')
  self.assertTrue(result['truncated'])
  self.assertGreater(len(result['functions']),0)
  self.assertGreater(len(result['occurrences']),0)
  self.assertLess(len(result['functions']),self.index.MAX_FUNCTIONS)
  self.assertLess(len(result['occurrences']),self.index.MAX_OCCURRENCES)
  self.assertLessEqual(len(raw),self.index.MAX_RESULT_BYTES)
  self.assertGreater(len(raw),self.index.MAX_RESULT_BYTES-5000)
 def test_byte_budget_is_shared_across_files_and_includes_utf8_envelope(self):
  folder=self.root/'Türkçe-"klasör"';folder.mkdir()
  self.index.TIME_LIMIT=10
  (folder/'a.m').write_text('function f\n'*5)
  first=self.index.scan(folder,(self.root,),'f')
  # Size relative to this machine's temporary path, so the second file is
  # reached even when the orchestrator uses a long macOS temp directory.
  limit=len(json.dumps(first,ensure_ascii=False).encode('utf-8'))+64
  self.index.MAX_RESULT_BYTES=limit
  for name in ('b.m','c.m'):(folder/name).write_text('function f\n'*5)
  result=self.index.scan(folder,(self.root,),'f')
  self.assertTrue(result['truncated'])
  self.assertGreaterEqual(result['scanned_files'],2)
  self.assertLessEqual(len(json.dumps(result,ensure_ascii=False).encode('utf-8')),limit)
  self.assertEqual(result['limits']['result_bytes'],limit)
 def test_standalone_parser_byte_cap_and_single_oversized_record(self):
  functions,uses,limited=parse_source('function '+('f'*1000),'probe.m',result_byte_limit=500)
  self.assertTrue(limited)
  self.assertEqual(functions,[])
  self.assertEqual(uses,[])



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
