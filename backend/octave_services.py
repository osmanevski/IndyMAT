"""Short-lived Octave documentation and project-local package operations."""
from __future__ import annotations
from backend.i18n import tr
import json, os, re, signal, subprocess, tempfile, threading, time, uuid
from pathlib import Path
from backend.kernel import cli_executable

NAME_RE=re.compile(r'^[A-Za-z][A-Za-z0-9_]*$')

def quote(value):return "'"+str(value).replace("'","''")+"'"

class OctaveServices:
    def __init__(self,root,runtime,executable):
        self.root,self.runtime=Path(root),Path(runtime)
        self.executable=cli_executable(executable)
        self.prefix=self.root/'.packages';self.archprefix=self.prefix/'.arch';self.registry=self.prefix/'octave_packages'
        self.runtime.mkdir(parents=True,exist_ok=True);self.prefix.mkdir(exist_ok=True);self.archprefix.mkdir(exist_ok=True)
        self.lock=threading.RLock();self.jobs={};self.proc=None;self.closed=False

    def _setup(self):
        return f"pkg('prefix',{quote(self.prefix)},{quote(self.archprefix)}); pkg('local_list',{quote(self.registry)});"

    def _run_json(self,body,cwd,timeout=10):
        with tempfile.TemporaryDirectory(prefix='octave-service-',dir=self.runtime) as folder:
            folder=Path(folder);result=folder/'result.json';script=folder/'request.m'
            script.write_text(self._setup()+body+f"\nfid=fopen({quote(result)},'w'); if fid>=0, fputs(fid,jsonencode(result)); fclose(fid); endif\n",encoding='utf-8')
            try:
                completed=subprocess.run([self.executable,'--no-gui','--quiet','--no-init-file','--no-site-file',str(script)],cwd=cwd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=timeout,start_new_session=True)
            except subprocess.TimeoutExpired as exc:raise TimeoutError(tr('The Octave help operation timed out.')) from exc
            if not result.exists():raise ValueError((completed.stdout or tr('Octave did not respond.')).strip()[-2000:])
            return json.loads(result.read_text(encoding='utf-8'))

    def help(self,name,cwd):
        if not isinstance(name,str) or not NAME_RE.fullmatch(name):raise ValueError(tr('Enter a valid function name.'))
        body=f"""
warning('off','Octave:shadowed-function'); addpath({quote(cwd)});
packages=pkg('list');
for k=1:numel(packages), try, pkg('load',packages{{k}}.name); catch, end_try_catch, endfor
name={quote(name)}; source=which(name); result=struct('found',false,'name',name,'text','','source',source);
if ~isempty(source)
  try, result.text=evalc('help(name)'); result.found=true; catch err, result.text=err.message; end_try_catch
endif
"""
        result=self._run_json(body,Path(cwd),8)
        source=result.get('source') or ''
        if not result.get('found'):
            return {'found':False,'name':name,'message':tr('{name} was not found.', name=name)}
        result['origin'],result['origin_label']=self._origin(source,Path(cwd))
        return result

    def _origin(self,source,cwd):
        try:path=Path(source).resolve()
        except (OSError,ValueError):path=None
        if path:
            try:
                relative=path.relative_to(self.prefix.resolve());part=relative.parts[0]
                package=part.rsplit('-',1)[0] if '-' in part else part
                return 'package',tr('Package: {package} — {path}', package=package, path=source)
            except ValueError:pass
            try:path.relative_to(cwd.resolve());return 'user',tr('User file: {path}', path=source)
            except ValueError:pass
        return 'core',tr('Octave core: {path}', path=source)

    def packages(self,loaded=()):
        body="""
items={}; packages=pkg('list');
for k=1:numel(packages)
  p=packages{k}; items{end+1}=struct('name',p.name,'version',p.version,'dir',p.dir,'archprefix',p.archprefix);
endfor
result=struct('packages',{items});
"""
        data=self._run_json(body,self.root,10);active=set(loaded);out=[]
        for item in data.get('packages',[]):out.append({'name':item['name'],'version':item['version'],'loaded':item['name'] in active})
        return out

    def start(self,action,name='',source=None):
        if action not in ('install','uninstall'):raise ValueError(tr('Invalid package operation.'))
        if not isinstance(name,str) or not NAME_RE.fullmatch(name):raise ValueError(tr('Enter a valid package name.'))
        local=None
        if source is not None:
            local=Path(source).resolve()
            if not local.is_file():raise ValueError(tr('Package archive not found.'))
        if action=='uninstall':self._check_uninstall(name)
        with self.lock:
            if self.closed:raise ValueError(tr('The package manager is closed.'))
            if self.proc and self.proc.poll() is None:raise ValueError(tr('Another package operation is in progress.'))
            job=uuid.uuid4().hex;self.jobs={job:{'job':job,'status':'running','action':action,'name':name,'output':'','started':time.time()}}
            thread=threading.Thread(target=self._manage,args=(job,action,name,local),daemon=True);thread.start()
            return job

    def _check_uninstall(self,name):
        body="""items={}; packages=pkg('list'); for k=1:numel(packages), p=packages{k}; items{end+1}=struct('name',p.name,'dir',p.dir,'archprefix',p.archprefix); endfor; result=struct('packages',{items});"""
        found=next((p for p in self._run_json(body,self.root,10).get('packages',[]) if p['name']==name),None)
        if not found:raise ValueError(tr('The package is not installed.'))
        base=self.prefix.resolve()
        for key in ('dir','archprefix'):
            value=found.get(key)
            if value:
                try:Path(value).resolve().relative_to(base)
                except ValueError:raise ValueError(tr('The package registry points outside the project folder; run scripts/relocate_packages.py first.'))

    def _manage(self,job,action,name,local):
        with tempfile.TemporaryDirectory(prefix='package-job-',dir=self.runtime) as folder:
            script=Path(folder)/'package.m'
            if action=='install':operation=f"pkg('install',{quote(local)})" if local else f"pkg('install','-forge',{quote(name)})"
            else:operation=f"pkg('uninstall',{quote(name)})"
            script.write_text(self._setup()+f"\ntry\n  {operation};\ncatch err\n  fprintf(2,'%s\\n',err.message); exit(1);\nend_try_catch\n",encoding='utf-8')
            try:
                proc=subprocess.Popen([self.executable,'--no-gui','--quiet','--no-init-file','--no-site-file',str(script)],cwd=self.root,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,start_new_session=True,bufsize=1)
                with self.lock:
                    self.proc=proc
                    if self.closed:self._terminate(proc)
                timer=threading.Timer(1200,lambda:self._terminate(proc));timer.daemon=True;timer.start()
                for line in proc.stdout:
                    if 'ignoring const execution_exception' in line:continue
                    with self.lock:
                        if job in self.jobs:self.jobs[job]['output']=(self.jobs[job]['output']+line)[-200000:]
                code=proc.wait();timer.cancel()
                proc.stdout.close()
                with self.lock:
                    state=self.jobs.get(job)
                    if state:state.update(status='done' if code==0 else 'failed',finished=time.time(),error=None if code==0 else tr('The package operation could not be completed. See the output for details.'))
            except Exception as exc:
                with self.lock:
                    state=self.jobs.get(job)
                    if state:state.update(status='failed',finished=time.time(),error=str(exc))
            finally:
                with self.lock:
                    if self.proc is locals().get('proc'):self.proc=None

    def status(self,job):
        with self.lock:
            if job not in self.jobs:raise ValueError(tr('Package job not found.'))
            return dict(self.jobs[job])

    @staticmethod
    def _terminate(proc):
        if proc.poll() is None:
            try:os.killpg(proc.pid,signal.SIGTERM)
            except ProcessLookupError:pass

    def close(self):
        with self.lock:self.closed=True;proc=self.proc
        if proc:
            self._terminate(proc)
            try:proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                try:os.killpg(proc.pid,signal.SIGKILL)
                except ProcessLookupError:pass
                proc.wait(timeout=2)
