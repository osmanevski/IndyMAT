import json,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'scripts'))
import compat

class CompatTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
 def tearDown(self):self.temp.cleanup()
 def cases(self,text):
  (self.root/'d').mkdir(exist_ok=True);(self.root/'d'/'a.m').write_text(text,encoding='utf-8');return compat.load_cases(self.root/'d')
 def test_case_files_are_parsed_and_validated(self):
  cases=self.cases('% Alan\n%% bir | sin cos | not\nx=sin(0)+cos(0);\n%% iki | :ozellik\ny=2;\n')
  self.assertEqual([(c['id'],c['functions'],c['note'],c['area']) for c in cases],[('bir',['sin','cos'],'not','Alan'),('iki',[':ozellik'],'','Alan')])
  self.assertEqual(cases[0]['code'],['x=sin(0)+cos(0);'])
  for bad in ('%% bir | sin\n','% Alan\nx=1;\n%% bir | sin\nsin(0);\n','% Alan\n%% bir\n','% Alan\n%% bir | sin\nsin(0);\n%% bir | cos\ncos(0);\n'):
   with self.assertRaises(ValueError):self.cases(bad)
  with self.assertRaisesRegex(ValueError,r'a\.m: bir:.*sin'):
   self.cases('% Alan\n%% bir | sin\nx=since(1);\n')
  self.assertEqual(self.cases('% Alan\n%% bir | containers.Map\nm=containers.Map();\n')[0]['functions'],['containers.Map'])
 def test_identifier_check_ignores_comments_and_literals_but_preserves_transpose(self):
  for code in ('x=1; % sin(0)',"x='sin(0)';",'x="sin(0)";',"x='it''s sin';",'x=1; % sin\nx=2;'):
   with self.subTest(code=code),self.assertRaisesRegex(ValueError,r'a\.m: bir:.*sin'):
    self.cases('% Alan\n%% bir | sin\n'+code+'\n')
  for expression in ("a'","a)'","a]'","a}'","a.'","a''"):
   self.assertIn('sin',compat.executable_text(expression+'; sin(0);'))
  self.cases("% Alan\n%% bir | sin\nx='a%''b'; sin(0); % comment\n")
  self.assertNotIn('hidden',compat.executable_text("x='a%''hidden'; sin(0);"))
 def test_case_hash_and_matlab_reference_require_current_body(self):
  case=self.cases('% Alan\n%% bir | sin | eski not\nx=sin(0);\n')[0]
  changed_note={**case,'note':'yeni not'};changed_code={**case,'code':['x=sin(1);']};changed_functions={**case,'functions':['sin','cos']}
  self.assertEqual(compat.case_hash(case),compat.case_hash(changed_note))
  self.assertNotEqual(compat.case_hash(case),compat.case_hash(changed_code))
  self.assertNotEqual(compat.case_hash(case),compat.case_hash(changed_functions))
  self.assertNotEqual(compat.case_hash(case),compat.case_hash({**case,'id':'iki'}))
  data={'schema':compat.MATLAB_SCHEMA,'matlab':'25.2.0.2998904 (R2025b)','durumlar':{'bir':compat.matlab_record(case,'OK')}}
  self.assertTrue(compat.matlab_reference_valid([case],data))
  self.assertEqual(compat.confirmed_matlab_cases([case],data),{'bir'})
  self.assertFalse(compat.matlab_reference_valid([changed_code],data))
  self.assertEqual(compat.confirmed_matlab_cases([changed_code],data),set())
  self.assertFalse(compat.matlab_reference_valid([case],{'durumlar':{'bir':'OK'}}))
  self.assertFalse(compat.matlab_reference_valid([case],{**data,'durumlar':{'bir':compat.matlab_record(case,'FAIL')}}))
  self.assertFalse(compat.matlab_reference_valid([case],{**data,'durumlar':{**data['durumlar'],'fazla':compat.matlab_record(case,'OK')}}))
  for metadata in ({'schema':99},{'schema':None},{'matlab':''},{'matlab':'R2025b'},{'matlab':'9.1.0'},{'matlab':'not MATLAB'}):
   with self.subTest(metadata=metadata):
    self.assertFalse(compat.matlab_reference_valid([case],{**data,**metadata}))
    self.assertEqual(compat.confirmed_matlab_cases([case],{**data,**metadata}),set())
 def test_fixture_tree_names_and_contents_invalidate_all_case_hashes(self):
  cases=self.cases('% Alan\n%% bir | sin\nsin(0);\n%% iki | cos\ncos(0);\n')
  fixtures=self.root/'fixtures';fixtures.mkdir();(fixtures/'nested').mkdir()
  first=fixtures/'a.m';second=fixtures/'nested'/'b.m'
  first.write_text('first');second.write_text('second')
  original=[compat.case_hash(case,fixtures) for case in cases]
  data={'schema':compat.MATLAB_SCHEMA,'matlab':'25.2.0.2998904 (R2025b)','durumlar':{case['id']:compat.matlab_record(case,'OK',fixtures) for case in cases}}
  self.assertTrue(compat.matlab_reference_valid(cases,data,fixtures))
  first.write_text('changed')
  self.assertTrue(all(old!=compat.case_hash(case,fixtures) for old,case in zip(original,cases)))
  self.assertEqual(compat.confirmed_matlab_cases(cases,data,fixtures),set())
  first.write_text('first');first.rename(fixtures/'renamed.m')
  self.assertFalse(compat.matlab_reference_valid(cases,data,fixtures))
  (fixtures/'renamed.m').rename(first)
  self.assertEqual(original,[compat.case_hash(case,fixtures) for case in cases])
  other=self.root/'other';other.mkdir();(other/'nested').mkdir()
  (other/'nested'/'b.m').write_text('second');(other/'a.m').write_text('first')
  self.assertEqual(compat.fixture_hash(fixtures),compat.fixture_hash(other))
  second.unlink()
  self.assertFalse(compat.matlab_reference_valid(cases,data,fixtures))
 def test_matlab_note_counts_only_matching_hashes(self):
  cases=self.cases('% Alan\n%% bir | sin\nsin(0);\n%% iki | cos\ncos(0);\n')
  fixtures=self.root/'yardimcilar';fixtures.mkdir()
  data={'schema':compat.MATLAB_SCHEMA,'matlab':'25.2.0.2998904 (R2025b)','durumlar':{'bir':compat.matlab_record(cases[0],'OK',fixtures),'iki':compat.matlab_record({**cases[1],'code':['cos(1);']},'OK',fixtures)}}
  (self.root/'matlab.json').write_text(json.dumps(data),encoding='utf-8')
  self.assertIn('1 / 2',compat.matlab_note(cases,self.root))
 def test_matlab_reference_writer_records_schema_release_and_fixture_hashes(self):
  cases=self.cases('% Alan\n%% bir | sin\nsin(0);\n')
  (self.root/'d').rename(self.root/'durumlar')
  fixtures=self.root/'yardimcilar';fixtures.mkdir();(fixtures/'helper.m').write_text('helper')
  reference=self.root/'referans';reference.mkdir();(reference/'a.txt').write_text('# Alan\nsin\n')
  version='25.2.0.2998904 (R2025b)';results={'bir':{'status':'OK','message':''}}
  with patch.object(compat,'matlab_executable',return_value='fake-matlab'),patch.object(compat,'execute',return_value=(results,version,{'sin':5})),patch('builtins.print'):
   self.assertEqual(compat.matlab_reference(self.root),0)
  raw=(self.root/'matlab.json').read_text();data=json.loads(raw)
  self.assertEqual(data['schema'],compat.MATLAB_SCHEMA)
  self.assertEqual(data['matlab'],version)
  self.assertTrue(compat.matlab_reference_valid(cases,data,fixtures))
  self.assertEqual(raw,json.dumps(data,ensure_ascii=False,indent=1,sort_keys=True)+'\n')
 def test_reference_merges_curated_areas_before_octave_groups(self):
  (self.root/'r').mkdir();(self.root/'r'/'10.txt').write_text('# Benim alan\nsin tf\n',encoding='utf-8')
  listing=self.root/'list.m';listing.write_text('case {"tf", "bode"}\n txt = check_package (fcn, "control", {"lti"});\npersistent list = {\n"yyaxis",\n"bode",\n};\n',encoding='utf-8')
  groups,core=compat.octave_reference(listing)
  reference=compat.build_reference(compat.curated_reference(self.root/'r'),groups,core)
  self.assertEqual(reference,{'sin':'Benim alan','tf':'Benim alan','bode':'Kontrol sistemleri','yyaxis':compat.UNSORTED})
  listing.write_text('nothing useful',encoding='utf-8')
  with self.assertRaises(ValueError):compat.octave_reference(listing)
 def test_real_octave_isolates_failing_missing_and_unparsable_cases(self):
  cases=self.cases('% Alan\n%% gecer | sin\nassert(sin(0)==0);\n%% kalir | cos\nassert(cos(0)==0);\n%% eksik | uy_olmayan_fonksiyon\nuy_olmayan_fonksiyon(1);\n%% bozuk | tan\nx = tan(;\n%% sonraki | tan\nassert(tan(0)==0);\n')
  (self.root/'run').mkdir();results=compat.run_cases(cases,self.root/'run',self.root)
  self.assertEqual({k:v['status'] for k,v in results.items()},{'gecer':'OK','kalir':'FAIL','eksik':'FAIL','bozuk':'FAIL','sonraki':'OK'})
  (self.root/'probe').mkdir();probed=compat.probe(['sin','cos','tan','uy_olmayan_fonksiyon','containers.Map'],self.root/'probe')
  self.assertTrue(probed['version'] and probed['unimplemented'].endswith('__unimplemented__.m'))
  self.assertEqual(probed['functions']['uy_olmayan_fonksiyon'],{'exist':0,'source':''})
  self.assertTrue(probed['functions']['containers.Map']['source'])
  reference={name:'Alan' for case in cases for name in case['functions']}
  rows=compat.classify(reference,cases,results,probed)
  self.assertEqual({k:v['status'] for k,v in rows.items()},{'sin':'doğrulandı','cos':'kısmi','uy_olmayan_fonksiyon':'yok','tan':'kısmi'})
 def test_execution_parser_uses_last_result_before_next_start_or_completion(self):
  cases=[{'id':'a'},{'id':'b'}];nonce='guvenli';other='sahte'
  output='\n'.join((f'@@START\t{nonce}\ta',f'@@CASE\t{nonce}\tb\tOK\tuydurma',f'@@CASE\t{other}\ta\tOK\tuydurma',f'@@CASE\t{nonce}\ta\tOK\t',f'@@CASE\t{nonce}\ta\tFAIL\tsonradan',f'@@START\t{nonce}\tb',f'@@CASE\t{nonce}\ta\tFAIL\tyanlis durum',f'@@CASE\t{nonce}\tb\tFAIL\tuydurma',f'@@CASE\t{nonce}\tb\tOK\t',f'@@VERSION\t{nonce}\t9.1.0',f'@@DONE\t{nonce}'))
  results,version,_=compat.parse_execution_output(output,cases,nonce)
  self.assertEqual(results['a'],{'status':'FAIL','message':'sonradan'})
  self.assertEqual(results['b'],{'status':'OK','message':''})
  self.assertEqual(version,'9.1.0')
 def test_execution_parser_marks_incomplete_without_overwriting_finished_results(self):
  cases=[{'id':'a'},{'id':'b'},{'id':'c'}];nonce='n'
  output=f'@@START\t{nonce}\ta\n@@CASE\t{nonce}\ta\tOK\t\n@@START\t{nonce}\tb\n@@CASE\t{nonce}\tb\tOK\t\n'
  for suffix,code in (('',0),(f'@@VERSION\t{nonce}\t9.1.0\n',0),(f'@@DONE\t{nonce}\n',0),('@@DONE\twrong\n',0),('',2),('',None)):
   with self.subTest(suffix=suffix,code=code):
    results,version,_=compat.parse_execution_output(output+suffix,cases,nonce,code)
    self.assertEqual({key:value['status'] for key,value in results.items()},{'a':'OK','b':'SONUCSUZ','c':'SONUCSUZ'})
    self.assertEqual(version,'')
  output+=f'@@START\t{nonce}\tc\n@@CASE\t{nonce}\tc\tFAIL\terror\n'
  for suffix,code in ((f'@@DONE\t{nonce}\n',0),(f'@@VERSION\t{nonce}\t9.1.0\n@@DONE\t{nonce}\n',2)):
   results,version,_=compat.parse_execution_output(output+suffix,cases,nonce,code)
   self.assertEqual({key:value['status'] for key,value in results.items()},{'a':'OK','b':'OK','c':'FAIL'})
   self.assertEqual(version,'')
 def test_real_runner_hides_nonce_and_overrides_forged_result(self):
  cases=self.cases("% Alan\n%% hidden | :runner\nassert(evalin('caller', 'exist(''nonce'',''var'')') == 0);\nassert(evalin('base', 'exist(''nonce'',''var'')') == 0);\ntry, evalin('caller','nonce'); error('nonce leaked'); catch err, assert(~strcmp(err.message,'nonce leaked')); end\n%% forged | :runner\nfprintf('@@CASE\\tknown\\tforged\\tOK\\t\\n');\nerror('terminal failure');\n")
  with patch.object(compat.secrets,'token_hex',return_value='known'):
   results,version,exists=compat.execute(cases,self.root,self.root,names=['sin'])
  self.assertEqual(results['hidden']['status'],'OK')
  self.assertEqual(results['forged']['status'],'FAIL')
  self.assertIn('terminal failure',results['forged']['message'])
  self.assertTrue(version)
  self.assertGreater(exists['sin'],0)
 def test_missing_function_takes_the_blame_and_features_follow_cases(self):
  cases=[{'id':'a','functions':['var','eksik'],'area':'A','note':'','code':[]},{'id':'b','functions':[':ozellik'],'area':'A','note':'','code':[]},{'id':'c','functions':['yontem'],'area':'A','note':'','code':[]}]
  results={'a':{'status':'FAIL','message':''},'b':{'status':'FAIL','message':''},'c':{'status':'OK','message':''}}
  probed={'functions':{'var':{'exist':2,'source':'/x/var.m'},'eksik':{'exist':0,'source':''},'yontem':{'exist':0,'source':''}},'methods':{'yontem':['string']}}
  rows=compat.classify({'var':'A','eksik':'A',':ozellik':'A','yontem':'A'},cases,results,probed)
  self.assertEqual({k:(v['status'],v['kind']) for k,v in rows.items()},{'var':('var','çekirdek'),'eksik':('yok',''),':ozellik':('yok','dil özelliği'),'yontem':('doğrulandı','yöntem:string')})
 def test_baseline_comparison_fails_only_on_regressions(self):
  baseline={'durumlar':{'a':'OK','b':'FAIL'},'mevcut':{'A':3}}
  self.assertEqual(compat.compare(baseline,{'durumlar':{'a':'OK','b':'OK'},'mevcut':{'A':4}})[0],[])
  problems,_=compat.compare(baseline,{'durumlar':{'a':'FAIL','b':'FAIL','c':'OK'},'mevcut':{'A':2}})
  self.assertEqual(len(problems),3)
 def test_recorded_matlab_reference_confirms_every_case(self):
  reference=compat.BASE/'matlab.json'
  if not reference.exists():self.skipTest('MATLAB referans koşusu kayıtlı değil')
  data=json.loads(reference.read_text(encoding='utf-8'));cases=compat.load_cases(compat.BASE/'durumlar')
  self.assertTrue(compat.matlab_reference_valid(cases,data),'matlab.json kimlik, karma veya OK durumu güncel değil')
 def test_project_baseline_covers_every_case(self):
  baseline=json.loads((compat.BASE/'baseline.json').read_text(encoding='utf-8'))
  self.assertEqual(sorted(baseline['durumlar']),sorted(case['id'] for case in compat.load_cases(compat.BASE/'durumlar')))


def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__=='__main__':unittest.main(verbosity=2)
