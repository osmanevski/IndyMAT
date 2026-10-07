#!/usr/bin/env python3
"""IndyMAT: offline local scientific IDE. Python 3.10+, GNU Octave."""
from __future__ import annotations
from backend.i18n import tr, set_language, get_language
import argparse, base64, hashlib, json, math, mimetypes, os, re, secrets, signal, sys, threading, time, webbrowser, shutil
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse, parse_qs, unquote
from backend.assistants import Assistants
from backend.assistant_bridge_service import BridgeService
from backend.kernel import Kernel
from backend.source_jobs import validate_source_context
from backend.files import Workspace
from backend.file_operations import FileOperations
from backend.workspace_actions import WorkspaceActions, MatTarget, variable_read_request, variable_write_request
from backend.lint import LintTimeout, OctaveLinter
from backend.symbols import SymbolIndex
from backend.octave_services import OctaveServices
from backend.publish import validate_published_html, sanitize_published_html, RASTER_TYPES, VIEW_HEADER_CSP
ROOT=Path(__file__).resolve().parent

WORKSPACE_CLASSES={'double','single','logical','char','int8','int16','int32','int64','uint8','uint16','uint32','uint64'}
WORKSPACE_INTEGER_LIMITS={'int8':(-128,127),'int16':(-32768,32767),'int32':(-2147483648,2147483647),'int64':(-9223372036854775808,9223372036854775807),'uint8':(0,255),'uint16':(0,65535),'uint32':(0,4294967295),'uint64':(0,18446744073709551615)}

def workspace_variable_name(value):
    if not isinstance(value,str) or len(value)>63 or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*',value) or value.startswith('__mf_'):
        raise ValueError(tr('Invalid or reserved variable name.'))
    return value

def workspace_variable_names(value,limit=500):
    if not isinstance(value,list) or len(value)>limit:raise ValueError(tr('Variable names must be a bounded list.'))
    names=[workspace_variable_name(item) for item in value]
    if len(set(names))!=len(names):raise ValueError(tr('Variable names must be unique.'))
    return names

def workspace_scalar(class_name,value):
    if not isinstance(class_name,str) or class_name not in WORKSPACE_CLASSES:raise ValueError(tr('This scalar class cannot be edited.'))
    if class_name=='logical':
        if not isinstance(value,bool):raise ValueError(tr('A logical scalar must be true or false.'))
    elif class_name=='char':
        if not isinstance(value,str) or len(value)!=1 or len(value.encode('utf-8'))!=1:raise ValueError(tr('The character value must be a single ASCII character.'))
    else:
        if isinstance(value,dict):
            if class_name not in ('double','single') or set(value)!={'special'} or value['special'] not in ('NaN','Inf','-Inf'):raise ValueError(tr('Invalid special floating-point value.'))
            return {'class':class_name,'value':value}
        if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value):raise ValueError(tr('The numeric value must be a finite scalar.'))
        if class_name=='single' and abs(value)>3.4028234663852886e38:raise ValueError(tr('The single value exceeds the class range.'))
        if class_name in WORKSPACE_INTEGER_LIMITS:
            low,high=WORKSPACE_INTEGER_LIMITS[class_name]
            if int(value)!=value or value<low or value>high:raise ValueError(tr('{class_name} value must be an integer within the class range.', class_name=class_name))
            if abs(value)>9007199254740991:raise ValueError(tr('This integer cannot be transferred losslessly through JSON.'))
    return {'class':class_name,'value':value}

def workspace_mat_path(workspace,name,must_exist):
    target=MatTarget(workspace,name)
    try:
        if must_exist and target.identity is None:raise FileNotFoundError(tr('MAT file not found.'))
        target.check()
        return target.path
    finally:target.close()

class PublishKernel(Kernel):
    """App-side completion fence; no changes to the persistent engine protocol.

    _collect eagerly finalises. snapshot/submit close the small interval between
    the base collector releasing its lock and this adapter reacquiring it: no
    observer sees publish success, and no next job replaces it, before sanitation.
    Reset may win that interval; then the old job is never installed.
    """
    def __init__(self,*args,publish_app,**kwargs):
        self.publish_app=publish_app
        super().__init__(*args,**kwargs)
    def _finish_publish(self):
        self.publish_app.finish_publish(self)
        if hasattr(self.publish_app,'workspace_actions'):self.publish_app.workspace_actions.finish(self)
        if hasattr(self.publish_app,'assistant_bridge'):self.publish_app.assistant_bridge.capture(self)
    def _collect(self,*args):
        super()._collect(*args)
        with self.lock:self._finish_publish()
    def snapshot(self):
        with self.lock:
            self._finish_publish()
            return super().snapshot()
    def submit(self,*args,**kwargs):
        with self.lock:
            self._finish_publish()
            return super().submit(*args,**kwargs)
    def _submit(self,*args,**kwargs):
        # Breakpoint jobs enter here directly; close the MAT completion gap too.
        if hasattr(self.publish_app,'workspace_actions'):self.publish_app.workspace_actions.finish(self)
        if hasattr(self.publish_app,'assistant_bridge'):self.publish_app.assistant_bridge.capture(self)
        return super()._submit(*args,**kwargs)
    def reset(self):
        with self.lock:
            generation=self.generation
            try:return super().reset()
            finally:
                self.publish_app.discard_publish_stages(self,generation)
                if hasattr(self.publish_app,'workspace_actions'):self.publish_app.workspace_actions.discard()
    def close(self):
        with self.lock:
            try:return super().close()
            finally:
                self.publish_app.discard_publish_stages(self)
                if hasattr(self.publish_app,'workspace_actions'):self.publish_app.workspace_actions.discard()

class App:
    def __init__(self,workspace,port,open_browser):
        self.workspace=Workspace(workspace)
        self.runtime=ROOT/'.matlab-free';self.runtime.mkdir(exist_ok=True)
        os.chmod(self.runtime,0o700)
        self.token=secrets.token_urlsafe(32)
        self.assistants=Assistants(self.workspace,self.token,describe=lambda:self.kernel.snapshot())
        self.history_path=self.runtime/'history.json'
        self.history_lock=threading.Lock();self.file_lock=threading.Lock()
        self.history_session=secrets.token_hex(8)
        self._load_history()
        self.publish_lock=threading.Lock();self.publish_renders={}
        self.workspace_actions=WorkspaceActions(self)
        self.workspace.source_private_roots=(self.runtime/'jobs',)
        self.server=ThreadingHTTPServer(('127.0.0.1',port),Handler)
        try:self.kernel=PublishKernel(ROOT,self.runtime/'jobs',self.workspace.root,publish_app=self)
        except Exception:
            self.server.server_close();raise
        self.linter=OctaveLinter(self.runtime/'lint',self.kernel.executable)
        self.symbols=SymbolIndex()
        self.octave_services=OctaveServices(ROOT,self.runtime/'services',self.kernel.executable)
        self.server.daemon_threads=True; self.server.app=self
        self.port=self.server.server_address[1]
        self.base=f'http://127.0.0.1:{self.port}'
        self.assistant_bridge=BridgeService(self)
        self.assistants.bridge=self.assistant_bridge
        self.open_browser=open_browser
    def close(self):self.assistants.close();self.octave_services.close();self.kernel.close();self.server.server_close()
    # Command history manager: persistence only; these methods never submit jobs.
    HISTORY_ENTRY_BYTES=64_000
    HISTORY_TOTAL_BYTES=1_000_000
    HISTORY_LOAD_BYTES=8_000_000
    def _load_history(self):
        parsed=False
        self.history_preserve=False
        try:
            with self.history_path.open('rb') as stream:content=stream.read(self.HISTORY_LOAD_BYTES+1)
            if len(content)>self.HISTORY_LOAD_BYTES:raise ValueError(tr('The history file exceeds the loading limit.'))
            raw=json.loads(content)
            parsed=True
        except (OSError,ValueError,TypeError):
            raw={}
            self.history_preserve=self.history_path.exists()
        migrated=parsed and isinstance(raw,list)
        if migrated:
            stamp=self.history_path.stat().st_mtime if self.history_path.exists() else time.time()
            raw={'entries':[{'id':secrets.token_hex(12),'code':code,'created':stamp,'session':'onceki'} for code in raw if isinstance(code,str)]}
        entries=raw.get('entries',[]) if isinstance(raw,dict) else []
        if not isinstance(entries,list):entries=[]
        self.history_entries=[]
        for entry in entries[-500:]:
            if not isinstance(entry,dict):continue
            code=entry.get('code');identifier=entry.get('id');created=entry.get('created');session=entry.get('session')
            if not isinstance(code,str) or not code.strip() or not isinstance(identifier,str) or not re.fullmatch(r'[a-f0-9]{24}',identifier):continue
            if isinstance(created,bool) or not isinstance(created,(int,float)) or not math.isfinite(created):continue
            if not isinstance(session,str) or not re.fullmatch(r'[a-z0-9]{1,32}',session):continue
            self.history_entries.append({'id':identifier,'code':code,'created':float(created),'session':session})
        self._bound_history_locked()
        # Install IDs immediately: two starts without any user mutation must
        # see exactly the same version-2 entries. Invalid files are preserved.
        if migrated or parsed and self.history_entries!=entries:
            with self.history_lock:self._write_history_locked()
    def _bound_history_locked(self):
        kept=[]
        total=len(b'{"version":2,"entries":[]}')
        for entry in reversed(self.history_entries[-500:]):
            code=entry['code']
            if len(code)>self.HISTORY_ENTRY_BYTES or len(code.encode('utf-8'))>self.HISTORY_ENTRY_BYTES:continue
            size=len(json.dumps(entry,ensure_ascii=False,separators=(',',':')).encode('utf-8'))
            if size>self.HISTORY_ENTRY_BYTES:continue
            if total+size+(1 if kept else 0)>self.HISTORY_TOTAL_BYTES:break
            total+=size+(1 if kept else 0)
            kept.append(entry)
        self.history_entries=list(reversed(kept))
        self.history=[entry['code'] for entry in self.history_entries]
    def _write_history_locked(self):
        self._bound_history_locked()
        data=json.dumps({'version':2,'entries':self.history_entries},ensure_ascii=False,separators=(',',':'))
        tmp=self.runtime/('.history-'+secrets.token_hex(12)+'.tmp')
        try:
            with tmp.open('x',encoding='utf-8') as stream:
                os.chmod(tmp,0o600)
                stream.write(data);stream.flush();os.fsync(stream.fileno())
            if self.history_preserve:
                # Oversized/corrupt pre-existing files are never silently lost.
                recovery=self.runtime/('history-recovery-'+secrets.token_hex(12)+'.json')
                os.link(self.history_path,recovery)
            os.replace(tmp,self.history_path)
            self.history_preserve=False
        finally:
            try:tmp.unlink()
            except FileNotFoundError:pass
    def history_codes(self):
        with self.history_lock:return list(self.history)
    def history_detail(self):
        with self.history_lock:return {'entries':[dict(entry) for entry in self.history_entries],'current_session':self.history_session}
    def record(self,code):
        # Oversized jobs still execute in full; never retain a truncated command
        # that could execute different code when recalled from history.
        if len(code)>self.HISTORY_ENTRY_BYTES or len(code.encode('utf-8'))>self.HISTORY_ENTRY_BYTES:return False
        with self.history_lock:
            if self.history and self.history[-1]==code:return True
            entry={'id':secrets.token_hex(12),'code':code,'created':time.time(),'session':self.history_session}
            if len(json.dumps(entry,ensure_ascii=False,separators=(',',':')).encode('utf-8'))>self.HISTORY_ENTRY_BYTES:return False
            self.history_entries.append(entry)
            self._write_history_locked()
            return True
    def mutate_history(self,request):
        if not isinstance(request,dict):raise ValueError(tr('The Command History request must be an object.'))
        action=request.get('action')
        with self.history_lock:
            if action=='delete':
                if set(request)!={'action','ids'}:raise ValueError(tr('Send only the action and history IDs.'))
                ids=request['ids']
                if not isinstance(ids,list) or not ids or len(ids)>500 or len(set(ids))!=len(ids) or any(not isinstance(value,str) or not re.fullmatch(r'[a-f0-9]{24}',value) for value in ids):raise ValueError(tr('Invalid history selection.'))
                known={entry['id'] for entry in self.history_entries}
                if not set(ids).issubset(known):raise ValueError(tr('Command History changed; refresh the selection.'))
                selected=set(ids);self.history_entries=[entry for entry in self.history_entries if entry['id'] not in selected]
            elif action=='clear':
                if set(request)!={'action','confirm'} or request.get('confirm') is not True:raise ValueError(tr('Explicit confirmation is required to clear Command History.'))
                self.history_entries=[]
            else:raise ValueError(tr('Invalid Command History action.'))
            self.history=[entry['code'] for entry in self.history_entries]
            self._write_history_locked()
            return {'ok':True,'count':len(self.history)}
    # End command history manager.
    def register_publish_render(self,job,path,generation,binding):
        capability=secrets.token_urlsafe(32)
        with self.publish_lock:
            self.publish_renders[capability]={'job':job,'path':str(path),'generation':generation,'hash':None,'binding':None,'initial':binding,'used':False,'finished':False,'created':time.monotonic()}
            if len(self.publish_renders)>32:
                oldest=sorted(self.publish_renders,key=lambda key:self.publish_renders[key]['created'])[:-32]
                for key in oldest:self.publish_renders.pop(key,None)
        return capability
    def finish_publish(self,kernel):
        # Caller holds kernel.lock. Every nested acquisition in the publish
        # subsystem is kernel -> publish -> file; never the reverse.
        state=kernel.state
        if state.get('kind')!='publish' or state.get('status') not in ('idle','dead'):return
        with self.publish_lock:
            ticket=next((t for t in self.publish_renders.values() if t['job']==state.get('job') and t['generation']==kernel.generation),None)
            if not ticket or ticket['finished']:return
            ticket['finished']=True
            folder=kernel.runtime/ticket['job']/'publish'
            try:
                if state.get('error') or state.get('status')!='idle':return
                report=state.get('publish') or {}
                if report.get('path')!=ticket['path']:raise ValueError(tr('The publish job returned an unexpected report path.'))
                with self.file_lock:
                    raw=read_report_asset(folder,Path(ticket['path']).name)
                    def image(name):
                        try:return inline_report_image(folder,name)
                        except FileNotFoundError:
                            # User-written raw HTML may refer to an existing
                            # raster image beside the destination report.
                            return inline_report_image(Path(ticket['path']).parent,name)
                    safe,images=sanitize_published_html(raw.decode('utf-8'),image_loader=image)
                    # A second pass checks caps on newly embedded images too.
                    safe,_=sanitize_published_html(safe)
                    result=self.workspace.replace_published(ticket['path'],safe,ticket['initial']['hash'],ticket['initial'])
                    ticket['binding']=result['binding']
                    report['images']=len(images)
                    report['staged']=False
            except Exception as exc:
                state['error']=tr('Could not create the HTML report safely: {error}', error=str(exc))
                state.pop('publish',None);ticket['used']=True
            finally:
                # Only our exact private staging directory, never user html/.
                remove_publish_stage(folder)
    def discard_publish_stages(self,kernel,generation=None):
        # After the old process is stopped, an abandoned raw staging directory
        # must not survive a reset/close. Caller holds kernel.lock.
        with self.publish_lock:
            with self.file_lock:
                for ticket in self.publish_renders.values():
                    if generation is None or ticket['generation']==generation:
                        ticket['used']=True;ticket['finished']=True
                        remove_publish_stage(kernel.runtime/ticket['job']/'publish')


def remove_publish_stage(folder):
    if folder.is_symlink():folder.unlink()
    elif folder.exists():shutil.rmtree(folder)


def read_report_asset(folder,name):
    """Read an asset through no-follow directory descriptors, with a byte cap."""
    folder=Path(folder)
    parts=Path(name).parts
    if not parts or Path(name).is_absolute() or any(p in ('.','..') for p in parts):raise ValueError(tr('Invalid report asset.'))
    fd=os.open('/',os.O_RDONLY|os.O_DIRECTORY)
    try:
        for part in folder.parts[1:]+parts[:-1]:
            next_fd=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd)
            os.close(fd);fd=next_fd
        raw,_=Workspace._report_read_at(fd,parts[-1],absent=True)
        if raw is None:raise FileNotFoundError(name)
        return raw
    finally:os.close(fd)


def inline_report_image(folder,name):
    kind=RASTER_TYPES[Path(name).suffix[1:].lower()]
    raw=read_report_asset(folder,name)
    return 'data:'+kind+';base64,'+base64.b64encode(raw).decode('ascii')

class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    @property
    def app(self):return self.server.app
    def send(self,status,data,kind='application/json; charset=utf-8',csp=None):
        if not isinstance(data,bytes):data=json.dumps(data,ensure_ascii=False).encode()
        self.send_response(status);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(data)))
        self.send_header('Cache-Control','no-store');self.send_header('X-Content-Type-Options','nosniff');self.send_header('Referrer-Policy','no-referrer')
        self.send_header('Content-Security-Policy',csp or "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; connect-src 'self'; frame-src blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'")
        self.end_headers()
        try:self.wfile.write(data)
        except (BrokenPipeError,ConnectionResetError):pass
    def validate(self,api=False):
        allowed={f'127.0.0.1:{self.app.port}',f'localhost:{self.app.port}'}
        if self.headers.get('Host') not in allowed:raise PermissionError(tr('Invalid Host.'))
        origin=self.headers.get('Origin')
        if origin and origin not in {f'http://{x}' for x in allowed}:raise PermissionError(tr('Invalid Origin.'))
        if self.headers.get('Sec-Fetch-Site')=='cross-site':raise PermissionError(tr('Cross-site access denied.'))
        if api and not secrets.compare_digest(self.headers.get('X-MF-Token',''),self.app.token):raise PermissionError(tr('Invalid session token. Reopen the app using the launcher.'))
        # Only authenticated API requests may change the process-wide language.
        # Serialize with kernel commands; this never writes to stdin by itself.
        languages=self.headers.get_all('X-MF-Language',[])
        if api and len(languages)==1 and languages[0] in ('en','tr'):
            with self.app.kernel.lock:
                if languages[0]!=get_language():set_language(languages[0])
    def do_GET(self):self.handle_request(False)
    def do_POST(self):self.handle_request(True)
    def handle_request(self,post):
        try:
            url=urlparse(self.path);path=unquote(url.path);params=parse_qs(url.query)
            if path.startswith('/api/bridge/'):
                self.validate(False)
                if 'Origin' in self.headers:raise PermissionError(tr('Browser Origin is not allowed on the session bridge.'))
                grant=self.app.assistant_bridge.authenticate(self.headers.get('X-IndyMAT-Bridge',''))
                if not post or path not in ('/api/bridge/tools','/api/bridge/call'):return self.send(404,{'error':tr('Not found')})
                size=int(self.headers.get('Content-Length','0'))
                if size<0 or size>150_000:raise ValueError(tr('The request is too large.'))
                data=json.loads(self.rfile.read(size))
                with grant['lock']:
                    if not grant['active']:raise PermissionError(tr('Invalid assistant session capability.'))
                    if path=='/api/bridge/tools':return self.send(200,self.app.assistant_bridge.tools(grant))
                return self.send(200,self.app.assistant_bridge.call(grant,data))
            self.validate(path.startswith('/api/'))
            if post:
                if not path.startswith('/api/'):return self.send(404,{'error':tr('Not found')})
                size=int(self.headers.get('Content-Length','0'))
                if size<0 or size>28_000_000:raise ValueError(tr('The request is too large.'))
                data=json.loads(self.rfile.read(size))
                return self.post(path,data)
            if path=='/api/assistant/providers':return self.send(200,self.app.assistants.providers())
            if path=='/api/assistant/models':return self.send(200,self.app.assistants.models(params.get('provider',[''])[0]))
            if path=='/api/assistant/events':return self.send(200,self.app.assistants.events(params.get('session',[''])[0],int(params.get('after',['0'])[0])))
            if path=='/api/state':
                with self.app.kernel.lock:
                    state=self.app.kernel.snapshot()
                    state['assistant_jobs']=self.app.assistant_bridge.visible_jobs(self.app.kernel)
                with self.app.file_lock:
                    if state['status']=='idle':self.app.workspace.follow(state.get('cwd',''))
                    state.update(self.app.workspace.info())
                state.update(epoch=self.app.kernel.generation)
                return self.send(200,state)
            if path=='/api/files':
                with self.app.file_lock:data=self.app.workspace.tree()
                return self.send(200,data)
            # Editor intelligence: bounded Python parsing of the current folder only.
            if path=='/api/symbols':
                if set(params)-{'name'} or len(params.get('name',[]))>1:raise ValueError(tr('Invalid symbol index request.'))
                name=params.get('name',[None])[0]
                with self.app.file_lock:data=self.app.symbols.scan(self.app.workspace.current,self.app.workspace.roots,name)
                return self.send(200,data)
            # End editor intelligence route.
            if path=='/api/file':
                with self.app.file_lock:data=self.app.workspace.read(params.get('path',[''])[0])
                return self.send(200,data)
            if path=='/api/history':return self.send(200,self.app.history_codes())
            if path=='/api/history-detail':return self.send(200,self.app.history_detail())
            if path=='/api/help':
                state=self.app.kernel.snapshot();return self.send(200,self.app.octave_services.help(params.get('name',[''])[0],state.get('cwd') or self.app.workspace.current))
            if path=='/api/packages':
                loaded=[p['name'] for p in self.app.kernel.snapshot().get('packages',[]) if p.get('loaded')]
                return self.send(200,{'packages':self.app.octave_services.packages(loaded)})
            if path=='/api/package-job':return self.send(200,self.app.octave_services.status(params.get('id',[''])[0]))
            if path=='/api/figure':
                from backend.figure_artifacts import read_figure_artifact, FigureJSONTooLarge
                job=params.get('job',[''])[0];file=params.get('file',[''])[0]
                import re
                if not re.fullmatch('[a-f0-9]{32}',job) or not re.fullmatch(r'figure-\d+\.(?:png|json)',file):raise ValueError(tr('Invalid figure.'))
                kind='application/json; charset=utf-8' if file.endswith('.json') else 'image/png'
                try:data=read_figure_artifact(self.app.runtime/'jobs',job,file)
                except FigureJSONTooLarge as e:
                    return self.send(413,{'error':'Figure JSON exceeds the byte limit.','reason_code':'json_budget','reason_args':e.reason_args})
                return self.send(200,data,kind)
            if path=='/api/download':
                with self.app.file_lock:f=self.app.workspace.path(params.get('path',[''])[0]);data=f.read_bytes()
                return self.send(200,data,'application/octet-stream')
            if path=='/api/published':
                with self.app.file_lock:
                    raw,binding=self.app.workspace.read_published(params.get('path',[''])[0])
                    folder=Path(binding['path']).parent
                    html,_=sanitize_published_html(raw.decode('utf-8'),viewer=True,image_loader=lambda name:inline_report_image(folder,name))
                return self.send(200,html.encode(),'text/html; charset=utf-8',VIEW_HEADER_CSP)
            if path=='/api/published-render':
                job=params.get('job',[''])[0];capability=params.get('render',[''])[0]
                with self.app.kernel.lock:
                    state=self.app.kernel.snapshot()
                    with self.app.publish_lock:
                        ticket=self.publish_ticket(job,capability,state,require_hash=False)
                        with self.app.file_lock:
                            raw,binding=self.app.workspace.read_published(ticket['path'],ticket['binding'])
                            ticket['hash']=binding['hash'];ticket['get_binding']=binding
                return self.send(200,raw,'text/html; charset=utf-8',"default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'; sandbox")
            if path.startswith('/api/'):return self.send(404,{'error':tr('Not found')})
            static=ROOT/'static';f=(static/('index.html' if path=='/' else path.lstrip('/'))).resolve()
            if not f.is_relative_to(static):raise PermissionError(tr('Invalid path.'))
            return self.send(200,f.read_bytes(),mimetypes.guess_type(f.name)[0] or 'application/octet-stream')
        except PermissionError as e:self.send(403,{'error':str(e)})
        except FileExistsError as e:
            data={'error':str(e)}
            if getattr(e,'code',None):data.update(code=e.code, **e.details)
            self.send(409,data)
        except FileNotFoundError:self.send(404,{'error':tr('File not found.')})
        except LintTimeout as e:self.send(408,{'error':str(e)})
        except TimeoutError as e:self.send(408,{'error':str(e)})
        except (ValueError,KeyError,TypeError,UnicodeError) as e:self.send(400,{'error':str(e)})
        except Exception as e:self.send(500,{'error':str(e)})
    def publish_ticket(self,job,capability,state,require_hash=True,claimed=False):
        # Caller holds kernel.lock and publish_lock (in that order).
        if not isinstance(job,str) or not isinstance(capability,str):raise PermissionError(tr('Invalid publish job capability.'))
        ticket=self.app.publish_renders.get(capability)
        if not ticket or ticket['job']!=job or ticket['used']!=claimed or ticket['generation']!=self.app.kernel.generation or not ticket['binding'] or (require_hash and not ticket['hash']):raise PermissionError(tr('Invalid publish job capability.'))
        report=state.get('publish') or {}
        if state.get('status')!='idle' or state.get('job')!=job or state.get('kind')!='publish' or state.get('error') or report.get('path')!=ticket['path']:raise PermissionError(tr('This publish job can no longer be written.'))
        return ticket
    def post(self,path,d):
        if path=='/api/assistant/start':
            facts=self.app.assistants.describe() if self.app.assistants.describe else {}
            with self.app.file_lock:result=self.app.assistants.start(d,facts=facts)
            return self.send(202,result)
        if path=='/api/assistant/approve':
            return self.send(200,self.app.assistant_bridge.approvals.decide(d))
        if path=='/api/assistant/remove':
            if not isinstance(d,dict) or set(d)!={'session'} or not isinstance(d['session'],str):raise ValueError(tr('Invalid assistant request.'))
            return self.send(200,self.app.assistants.remove(d['session']))
        if path=='/api/assistant/stop':
            if not isinstance(d,dict) or set(d)!={'session'} or not isinstance(d['session'],str):raise ValueError(tr('Invalid assistant request.'))
            return self.send(200,self.app.assistants.stop(d['session']))
        k=self.app.kernel;w=self.app.workspace
        # Command Window and Command History routes. History mutations only
        # replace the private history file; location resolution only reads a
        # bounded .m path. Neither route can submit code or write to stdin.
        if path=='/api/history':return self.send(200,self.app.mutate_history(d))
        if path=='/api/error-location':
            if not isinstance(d,dict) or set(d)!={'path','line'}:raise ValueError(tr('Send only the file path and line.'))
            candidate=d['path'];line=d['line']
            if not isinstance(candidate,str) or not candidate or len(candidate)>4096:raise ValueError(tr('Invalid error location.'))
            if isinstance(line,bool) or not isinstance(line,int) or line<1 or line>10_000_000:raise ValueError(tr('Invalid line number.'))
            names=[candidate]
            if '>' in candidate:names.append(candidate.split('>',1)[0])
            names += [name+'.m' for name in list(names) if not Path(name).suffix]
            for name in names:
                try:f=w.path(name)
                except FileNotFoundError:continue
                if f.is_file() and f.suffix.lower()=='.m':return self.send(200,{'path':str(f),'line':line})
            raise FileNotFoundError(tr('The file at the error location was not found.'))
        # End Command Window and Command History routes.
        if path=='/api/folder':
            with self.app.file_lock:target=w.folder(d['path'])
            job=k.submit(f"cd({k.quote(target)});",'code')
            with self.app.file_lock:w.current=target
            return self.send(202,{'job':job,'path':str(target)})
        if path=='/api/execute':
            mode=d.get('mode','code');arg=d.get('argument','');code=d.get('code','')
            if mode not in ('code','file','inspect','rotate'):raise ValueError(tr('Invalid operation.'))
            if not isinstance(code,str) or len(code)>2_000_000:raise ValueError(tr('The code is too large.'))
            if mode=='file':
                f=w.path(arg)
                if not f.is_file() or f.suffix!='.m':raise ValueError(tr('Select a .m file.'))
                arg=str(f)
            source_context=None
            if 'adapt_editor_literals' in d and type(d['adapt_editor_literals']) is not bool:
                raise ValueError(tr('Invalid editor source context.'))
            if d.get('adapt_editor_literals') is True:
                with self.app.file_lock:
                    source_context=validate_source_context(d.get('source_context'),code,mode,w)
            elif 'source_context' in d:
                raise ValueError(tr('Invalid editor source context.'))
            if source_context is not None:
                with k.lock:
                    job=k.submit(code,mode,arg,min(3600,max(0,float(d.get('timeout',0)))),source_context=source_context)
                    adapter=k.snapshot().get('source_adapter')
                return self.send(202,{'job':job,'source_adapter':adapter})
            job=k.submit(code,mode,arg,min(3600,max(0,float(d.get('timeout',0)))))
            if mode=='code' and code.strip() and d.get('history') is True:
                return self.send(202,{'job':job,'history_recorded':self.app.record(code)})
            return self.send(202,{'job':job})
        # Workspace manager: validated typed jobs in the persistent Octave process.
        if path=='/api/workspace':
            if not isinstance(d,dict):raise ValueError(tr('The Workspace request must be an object.'))
            action=d.get('action')
            if action=='rename':
                request={'old_name':workspace_variable_name(d.get('old_name')),'new_name':workspace_variable_name(d.get('new_name'))}
                mode='workspace-rename'
            elif action=='assign-scalar':
                request={'name':workspace_variable_name(d.get('name')),**workspace_scalar(d.get('class'),d.get('value'))}
                mode='workspace-assign-scalar'
            elif action=='clear-names':
                if d.get('confirm') is not True:raise ValueError(tr('Explicit confirmation is required to delete variables.'))
                request={'names':workspace_variable_names(d.get('names'))}
                if not request['names']:raise ValueError(tr('No variables selected for deletion.'))
                mode='workspace-clear-names'
            elif action in ('save','load-inspect','load'):
                job=self.app.workspace_actions.submit(d,workspace_variable_names)
                return self.send(202,{'job':job})
            else:
                raise ValueError(tr('Invalid Workspace operation.'))
            job=k.workspace_action(mode,request)
            return self.send(202,{'job':job})
        # End workspace manager routes.
        # Variables editor: bounded typed range jobs in the persistent session.
        if path=='/api/variable':
            if not isinstance(d,dict):raise ValueError(tr('The variable request must be an object.'))
            action=d.get('action')
            request=variable_read_request(d) if action=='read' else variable_write_request(d) if action=='write' else None
            if request is None:raise ValueError(tr('Invalid variable operation.'))
            with k.lock:
                state=k.snapshot()
                if state['status']=='paused':raise ValueError(tr('The Variables editor is read-only while debugging is paused.'))
                if state['status']!='idle' or state.get('waiting_input'):raise ValueError(tr('Finish the running operation or input request first.'))
                if request['epoch']!=k.generation:raise ValueError(tr('The Octave session changed; reopen the variable.'))
                if action=='write' and request['read_job']!=state.get('job'):raise ValueError(tr('The variable changed since the last read; refresh the page.'))
                job=k.workspace_action('variable-'+action,request)
            return self.send(202,{'job':job})
        # End variables editor routes.
        if path in ('/api/profile','/api/publish'):
            if not isinstance(d,dict) or set(d)!={'path'}:raise ValueError(tr('Send only the file path.'))
            f=w.path(d['path'])
            if not f.is_file() or f.suffix.lower()!='.m':raise ValueError(tr('Select a saved .m file.'))
            if path=='/api/publish':
                target=f.parent/'html'/(f.stem+'.html')
                # Hold the kernel lock through ticket registration, so even a
                # very fast completion cannot outrun capability registration.
                with k.lock:
                    k.snapshot()  # Finalise any preceding publish before hashing this target.
                    with self.app.file_lock:_,binding=w.read_published(str(target),absent=True,create=True)
                    job=k.submit(mode='publish',argument=str(f),timeout=3600)
                    response={'job':job,'render':self.app.register_publish_render(job,target,k.generation,binding)}
            else:
                job=k.submit(mode='profile',argument=str(f),timeout=3600);response={'job':job}
            return self.send(202,response)
        if path=='/api/published-render':
            if not isinstance(d,dict) or set(d)!={'job','render','html'}:raise ValueError(tr('Send only the publish job, capability, and HTML.'))
            with k.lock:
                state=k.snapshot()
                with self.app.publish_lock:
                    ticket=self.publish_ticket(d['job'],d['render'],state)
                    ticket['used']=True  # A failed/replayed attempt cannot race another POST.
            html=validate_published_html(d['html'])
            with k.lock:
                state=k.snapshot()
                with self.app.publish_lock:
                    checked=self.publish_ticket(d['job'],d['render'],state,claimed=True)
                    if checked is not ticket:raise PermissionError(tr('The publish job capability changed.'))
                    with self.app.file_lock:
                        result=w.replace_published(ticket['path'],html,ticket['hash'],ticket['get_binding'])
                        ticket['binding']=result.pop('binding')
            return self.send(200,result)
        if path=='/api/lint':
            if not isinstance(d,dict) or set(d)!={'code'}:raise ValueError(tr('Send only the code text.'))
            return self.send(200,{'issues':self.app.linter.check(d['code'])})
        if path=='/api/package-session':
            job=k.package(d.get('action'),d.get('name'));return self.send(202,{'job':job})
        if path=='/api/package-admin':
            if d.get('confirm') is not True:raise ValueError(tr('Explicit confirmation is required to install or uninstall a package.'))
            action=d.get('action');name=d.get('name','');source=d.get('source','forge');local=None
            if action=='uninstall' and any(p.get('name')==name and p.get('loaded') for p in k.snapshot().get('packages',[])):raise ValueError(tr('Unload the package in this Octave session before uninstalling it.'))
            if action=='install' and source=='local':local=w.path(d.get('path',''))
            elif action=='install' and source!='forge':raise ValueError(tr('Invalid package source.'))
            job=self.app.octave_services.start(action,name,local);return self.send(202,{'job':job})
        if path=='/api/stop':k.interrupt();return self.send(200,{'ok':True})
        if path=='/api/reset':k.reset();return self.send(200,{'ok':True})
        if path=='/api/breakpoint':
            f=w.path(d.get('path',''));line=int(d.get('line',0));action=d.get('action')
            if action is None:
                enabled=d.get('enabled')
                if not isinstance(enabled,bool):raise ValueError(tr('Invalid breakpoint state.'))
                job=k.breakpoint(f,line,enabled,d.get('condition',''));return self.send(202,{'job':job,'file':str(f),'line':line,'enabled':enabled})
            if set(d)-{'path','line','action','condition','enabled'}:raise ValueError(tr('Invalid breakpoint field.'))
            enabled=d.get('enabled')
            if enabled is not None and not isinstance(enabled,bool):raise ValueError(tr('Invalid breakpoint state.'))
            job=k.configure_breakpoint(f,line,action,d.get('condition'),enabled);return self.send(202,{'job':job,'file':str(f),'line':line,'action':action})
        # Debugger panel routes: all path resolution remains inside Workspace;
        # Kernel applies the real-pause and closed-command checks.
        if path=='/api/breakpoints-clear':
            if d:raise ValueError(tr('This operation does not accept fields.'))
            job=k.clear_breakpoints();return self.send(202,{'job':job})
        if path=='/api/run-to-cursor':
            if set(d)!={'path','line'}:raise ValueError(tr('Send only the file path and line.'))
            f=w.path(d['path']);line=int(d['line']);k.run_to_cursor(f,line);return self.send(200,{'ok':True,'file':str(f),'line':line})
        if path=='/api/debug':
            k.debug(d.get('command',''),d.get('code',''));return self.send(200,{'ok':True})
        if path=='/api/input':
            text=d.get('text','')
            if '\n' in text or '\r' in text:raise ValueError(tr('Use a single line of input.'))
            k.input(text);return self.send(200,{'ok':True})
        # Current Folder operations: filesystem work plus identified breakpoint
        # maintenance when needed; never change the session cwd.
        if path=='/api/file-operation':
            # Same lock order as publish: prevent jobs, other browsers and
            # saves from interleaving with validation and filesystem mutation.
            with k.lock:
                state=k.snapshot()
                operation=d.get('operation') if isinstance(d,dict) else None
                if operation!='inspect' and (state.get('status')!='idle' or state.get('waiting_input')):
                    raise ValueError(tr('Finish the Octave operation before changing files; the session cannot be running, paused, or waiting for input.'))
                with self.app.file_lock:
                    if operation=='rename':
                        requested_source=str(FileOperations(w).path(d.get('source')))
                        if requested_source in getattr(k,'breakpoints',{}):FileOperations.breakpoint_name(d.get('name'))
                    result=w.operate(d,state.get('cwd') or str(w.current))
                    if operation in ('rename','move','trash'):
                        try:result['breakpoint_job']=k.relocate_file_breakpoints(result['source'],None if operation=='trash' else result['path'])
                        except Exception as exc:
                            # The filesystem operation already succeeded. The
                            # caller must still receive the new tab/draft path.
                            result['warning']=tr('The file operation completed; the item is now at “{path}”. Could not apply breakpoints: {error}', path=result['path'], error=exc)
            return self.send(200,result)
        if path=='/api/file':
            with self.app.file_lock:result=w.save(d['path'],d['content'],d.get('hash'))
            return self.send(200,result)
        if path=='/api/import':
            with self.app.file_lock:result=w.save(d['path'],base64.b64decode(d['data'],validate=True),None,True)
            return self.send(200,result)
        if path=='/api/shutdown':
            self.send(200,{'ok':True});threading.Thread(target=self.server.shutdown,daemon=True).start();return
        self.send(404,{'error':tr('Not found')})

def main():
    parser=argparse.ArgumentParser(description=tr('IndyMAT local scientific environment'))
    parser.add_argument('--port',type=int,default=8769);parser.add_argument('--workspace',default=str(ROOT/'workspace'));parser.add_argument('--no-browser',action='store_true')
    args=parser.parse_args()
    import fcntl
    (ROOT/'.matlab-free').mkdir(exist_ok=True)
    lock=open(ROOT/'.matlab-free/server.lock','w')
    try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    except BlockingIOError:
        print(tr('IndyMAT is already running.'));
        try:
            launch=json.loads((ROOT/'.matlab-free/launch.json').read_text())
            if not args.no_browser:webbrowser.open(launch['url'])
        except (OSError,ValueError):pass
        return 0
    try:app=App(args.workspace,args.port,not args.no_browser)
    except Exception as e:print(tr('Could not start: {error}', error=e),file=sys.stderr);return 1
    # Token only in a private local launch file and URL fragment, never access logs.
    launch={'url':app.base+'/#'+app.token,'port':app.port,'pid':os.getpid()}
    launchfile=ROOT/'.matlab-free/launch.json';launchfile.write_text(json.dumps(launch));os.chmod(launchfile,0o600)
    print(tr('IndyMAT is running: {url}\nPress Ctrl+C to close.\n', url=app.base),flush=True)
    if app.open_browser:webbrowser.open(launch['url'])
    def stop(signum,frame):threading.Thread(target=app.server.shutdown,daemon=True).start()
    signal.signal(signal.SIGINT,stop);signal.signal(signal.SIGTERM,stop)
    try:app.server.serve_forever(poll_interval=.2)
    finally:app.close();launchfile.unlink(missing_ok=True);lock.close()

if __name__=='__main__':sys.exit(main())
