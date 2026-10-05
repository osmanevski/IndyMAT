"""Run real engine, file, HTTP and browser checks against temporary local servers.

Test files are discovered, so a feature adds tests by adding files, never by editing this script:
tests/test_*.py and tests/*_unit.cjs run without a server; tests/http*.cjs and tests/ui*.cjs each
run against their own freshly started server, so one file's leftovers cannot fail another.
"""
import json,os,subprocess,sys,time,urllib.request
from pathlib import Path
root=Path(__file__).resolve().parents[1]
launchfile=root/'.matlab-free/launch.json'
def request_shutdown():
 if not launchfile.exists():return
 d=json.loads(launchfile.read_text());base,token=d['url'].split('#')
 try:urllib.request.urlopen(urllib.request.Request(base+'api/shutdown',b'{}',{'X-MF-Token':token,'Content-Type':'application/json'}),timeout=4).read()
 except OSError:pass
 for _ in range(60):
  if not launchfile.exists():return
  time.sleep(.1)
def discover(pattern,first):
 names=sorted(path.name for path in (root/'tests').glob(pattern))
 return [f'tests/{name}' for name in sorted(names,key=lambda name:(name!=first,name))]
def with_server(command,timeout):
 proc=subprocess.Popen([sys.executable,'app.py','--no-browser','--port','0'],cwd=root)
 try:
  for _ in range(200):
   if launchfile.exists():break
   if proc.poll() is not None:raise RuntimeError('server exited')
   time.sleep(.1)
  d=json.loads(launchfile.read_text());base,token=d['url'].split('#')
  for _ in range(300):
   req=urllib.request.Request(base+'api/state',headers={'X-MF-Token':token})
   state=json.loads(urllib.request.urlopen(req,timeout=3).read())
   if state['status']=='idle':break
   time.sleep(.1)
  else:raise RuntimeError('Octave startup timeout')
  subprocess.run(command,cwd=root,check=True,timeout=timeout)
 finally:
  request_shutdown()
  try:proc.wait(timeout=8)
  except subprocess.TimeoutExpired:proc.terminate();proc.wait(timeout=8)
request_shutdown()
if '--web-only' not in sys.argv:
 # The project root is put on the import path so a test file need not set it up itself.
 env={**os.environ,'PYTHONPATH':os.pathsep.join(filter(None,[str(root),os.environ.get('PYTHONPATH')]))}
 for name in discover('test_*.py','test_files.py'):subprocess.run([sys.executable,name],cwd=root,check=True,timeout=300,env=env)
 for name in discover('*_unit.cjs',''):subprocess.run(['node',name],cwd=root,check=True,timeout=120)
 subprocess.run([sys.executable,'scripts/compat.py','--check'],cwd=root,check=True,timeout=300)
for name in discover('http*.cjs','http.cjs'):with_server(['node',name],120)
for name in discover('ui*.cjs','ui.cjs'):with_server(['node',name],180)
print('ALL CHECKS PASSED')
