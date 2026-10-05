"""A single persistent, trusted local Octave session. No emulated evaluator."""
from __future__ import annotations
from backend.i18n import tr, get_language
from backend.source_adapter import AdapterProfile, PackageSupport, adapt_source, verify_package
from backend.source_jobs import map_error
import codecs, json, os, queue, re, shutil, signal, subprocess, threading, time, uuid
from pathlib import Path
import sys
# Octave's Qt runtime registers the session as a macOS Dock app ("octave-gui"). Qt honours this
# variable only when the process is not the direct child of a running parent, so on macOS the
# session is started through a helper that forks, execs Octave and exits (see _Detached).
os.environ.setdefault('QT_MAC_DISABLE_FOREGROUND_APPLICATION_TRANSFORM','1')

def cli_executable(executable):
    """Prefer Qt-free octave-cli for short-lived helper processes: faster and never in the Dock."""
    found=shutil.which(str(executable)) or str(executable);cli=Path(found).with_name('octave-cli')
    return str(cli) if Path(found).name=='octave' and cli.exists() else str(executable)

_OCTAVE_KEYWORDS=frozenset('''__FILE__ __LINE__ break case catch classdef continue do else elseif end end_try_catch
 end_unwind_protect endarguments endclassdef endenumeration endevents endfor endfunction endif endmethods endparfor
 endproperties endspmd endswitch endwhile for function global if otherwise parfor persistent return spmd switch try
 until unwind_protect unwind_protect_cleanup while'''.split())

class _Detached:
    """Popen-like handle for an Octave process reparented to launchd; same pipes, session and process group."""
    _HELPER="import os,sys\nif os.fork():os._exit(0)\nos.execvp(sys.argv[1],sys.argv[1:])"
    def __init__(self,argv,cwd,env=None):
        # macOS gives a GUI process the identity named by __CFBundleIdentifier, which a terminal or app sets
        # for its children; Octave would then appear in the Dock under the launcher's icon (Ghostty).
        env={k:v for k,v in (os.environ if env is None else env).items() if k!='__CFBundleIdentifier'}
        helper=subprocess.Popen([sys.executable,'-c',self._HELPER,*argv],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,cwd=cwd,env=env,start_new_session=True,bufsize=0)
        helper.wait();self.stdin,self.stdout,self.pid=helper.stdin,helper.stdout,helper.pid
    def poll(self):
        try:os.killpg(self.pid,0)
        except ProcessLookupError:return 0
        except PermissionError:pass
        return None
    def wait(self,timeout=None):
        deadline=None if timeout is None else time.monotonic()+timeout
        while self.poll() is None:
            if deadline is not None and time.monotonic()>=deadline:raise subprocess.TimeoutExpired('octave',timeout)
            time.sleep(.02)
        return 0

class Kernel:
    # Class-level default: kernels assembled without __init__ (test doubles) still have the field.
    run_to_cursor_state = None

    def __init__(self, root, runtime, workspace, executable=None):
        self.root, self.runtime, self.workspace = map(Path, (root, runtime, workspace))
        self.executable = executable or os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave') or '/opt/homebrew/bin/octave'
        self.lock = threading.RLock()
        self.proc = None
        self.events = queue.Queue(maxsize=128)
        self.job = None
        self.breakpoints = {}
        self.breakpoint_jobs = {}
        self.run_to_cursor_state = None
        self.debug_frame_target = None
        self.generation = 0
        self.closed = False
        self.state = {'status':'starting','output':'','variables':[], 'figures':[], 'error':None, 'version':'', 'packages':[], 'cwd':str(self.workspace),'waiting_input':False,'console_clear':0,'debug':None}
        self.runtime.mkdir(parents=True,exist_ok=True)
        (self.root/'.packages'/'.arch').mkdir(parents=True,exist_ok=True)
        self.start()

    @staticmethod
    def quote(s): return "'" + str(s).replace("'", "''") + "'"

    # Protocol lines never name anything a user variable can shadow. A function
    # handle literal, (@name)(...), resolves the function even when a variable of
    # that name exists (measured with Octave 11.3, also at the debug> prompt and
    # for the reserved entry name itself). Command syntax and plain calls would
    # resolve to the variable. Octave helpers called this way run in their own
    # function workspace; user code reaches the base or paused frame by evalin.
    # Limit: user function FILES that shadow built-ins (fprintf.m, rehash.m ...)
    # are not defended against.
    @classmethod
    def call(cls,name,*args): return f"(@{name})({','.join(str(a) if isinstance(a,int) else cls.quote(a) for a in args)});"

    def _path_commands(self): return self.call('addpath',self.root/'octave')+' '+self.call('addpath',self.root/'octave'/'compat')

    def _marker_command(self,text): return self.call('__mf_marker__',text)

    def _write_command(self, proc, command):
        """Carry a language change on an existing protocol command, never an input reply.

        Header updates only change Python state. A running job keeps its
        language until the next legitimate command boundary (including debugger
        controls, inspections, and interrupt recovery). The extra line is sent
        only when the language differs from what this Octave process was last
        told: a second line on every command would give an interrupt a window
        in which it cancels the language line and leaves the real command
        running. The function-handle literal is safe when a user variable
        shadows setenv. No assignments or new variables enter the base or
        paused workspace.
        """
        with self.lock:
            wanted = get_language()
            if wanted == getattr(self, '_octave_language', None):
                proc.stdin.write(command.encode())
                return
            self._octave_language = wanted
            # Keep control statements on their own line: a pending prompt-level
            # return can skip the rest of a compound command before dbcont.
            proc.stdin.write((self.call('setenv', 'INDYMAT_LANGUAGE', wanted) + '\n' + command).encode())

    def start(self):
        self.generation += 1
        self.events = queue.Queue(maxsize=128)
        argv=[self.executable, '--no-gui', '--quiet', '--no-init-file', '--no-site-file', '--no-history', '--no-line-editing', '--interactive']
        self._octave_language=get_language()
        env=os.environ.copy();env['INDYMAT_LANGUAGE']=self._octave_language
        if sys.platform=='darwin':self.proc=_Detached(argv,self.workspace,env)
        else:self.proc = subprocess.Popen(argv,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            cwd=self.workspace, env=env, start_new_session=True, bufsize=0)
        threading.Thread(target=self._reader,args=(self.proc,self.events),daemon=True).start()
        init = "more off; page_screen_output(false); page_output_immediately(true); crash_dumps_octave_core(false); warning('off','Octave:shadowed-function'); "
        # No user variable exists yet, so plain names are safe on this one line.
        init += f"addpath({self.quote(self.root/'octave')}); set(0,'defaultfigurevisible','off'); "
        init += f"addpath({self.quote(self.root/'octave'/'compat')}); "
        # Relocation jobs skip the path commands (see _submit), so they enter through a handle stored now;
        # every helper they need is a subfunction of the entry file and resolves without the load path.
        init += "setappdata(0,'__mf_entry__',@__mf_execute__); "
        if (self.root/'.packages'/'octave_packages').exists():
            init += f"pkg('prefix',{self.quote(self.root/'.packages')},{self.quote(self.root/'.packages'/'.arch')}); pkg('local_list',{self.quote(self.root/'.packages'/'octave_packages')}); "
            init += "try; pkg load control signal datatypes; catch; end; try; pkg load statistics; catch; end; "
        init += "set(0,'defaultaxesfontname','Helvetica'); set(0,'defaulttextfontname','Helvetica'); PS1(''); PS2(''); clear ans; "
        self._write_command(self.proc, init + '\n')
        self._submit('', 'code', '', 30, initializing=True)

    def _reader(self,proc,events):
        try:
            while True:
                data=os.read(proc.stdout.fileno(),8192)
                if not data: break
                events.put(data)
        finally: events.put(None)

    def snapshot(self):
        with self.lock:
            state=json.loads(json.dumps(self.state))
            if state['status'] in ('starting','running','stopping'):state['elapsed']=round(time.time()-state['started'],2)
            state['breakpoints']=[{'file':path,'lines':sorted(line for line,item in points.items() if item['enabled']),'breakpoints':[{'line':line,**item} for line,item in sorted(points.items())]} for path,points in sorted(self.breakpoints.items())]
            if self.run_to_cursor_state:state['run_to_cursor']={'file':self.run_to_cursor_state['file'],'line':self.run_to_cursor_state.get('actual_line') or self.run_to_cursor_state['line'],'continued':self.run_to_cursor_state.get('continued',False)}
            return state

    def submit(self, code='', mode='code', argument='', timeout=0, *, source_context=None):
        with self.lock:
            if self.state['status'] in ('running','starting','stopping','paused'): raise ValueError(tr('Stop the running operation first.'))
            if not self.proc or self.proc.poll() is not None: raise ValueError(tr('Octave closed. Reset the session.'))
            return self._submit(code,mode,argument,timeout,source_context=source_context)

    def _source_package(self):
        # The previous completed job records the live path, resolution and cwd.
        # Reverify every opt-in job; package/path changes never reuse a capability.
        try:
            environment=json.loads((self.runtime/self.job/'source-environment.json').read_text())
            setup=self.call('path',environment['path'])
            return verify_package(environment['constructor'],executable=cli_executable(self.executable),
                                  setup=setup,cwd=environment['cwd'])
        except (OSError,ValueError,KeyError,TypeError):
            return PackageSupport(reason='Live constructor/package verification is unavailable.')

    def package(self,action,name):
        import re
        if action not in ('load','unload') or not isinstance(name,str) or not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*',name):raise ValueError(tr('Invalid package operation.'))
        argument=json.dumps({'action':action,'name':name,'prefix':str(self.root/'.packages'),'archprefix':str(self.root/'.packages'/'.arch'),'registry':str(self.root/'.packages'/'octave_packages')})
        return self.submit(mode='package',argument=argument)

    def workspace_action(self,mode,request):
        allowed={'workspace-rename','workspace-assign-scalar','workspace-clear-names','workspace-save','workspace-load-inspect','workspace-load','variable-read','variable-write'}
        if mode not in allowed or not isinstance(request,dict):raise ValueError(tr('Invalid Workspace operation.'))
        with self.lock:
            if self.state['status']=='paused':raise ValueError(tr('Workspace is read-only while debugging is paused.'))
            return self.submit(mode=mode,argument=json.dumps(request,ensure_ascii=False,separators=(',',':')))

    def _submit(self,code,mode,argument,timeout,initializing=False,apply_breakpoints=True,source_context=None):
        adaptation=None
        original=code
        if source_context is not None:
            source_context=json.loads(json.dumps(source_context))
            if mode!='code' or initializing:raise ValueError(tr('Source adaptation is limited to editor selections and sections.'))
            if adapt_source(source_context['document'],source_context['span']).generated_text!=code:
                raise ValueError(tr('Editor source span does not match the submitted code.'))
            adaptation=adapt_source(source_context['document'],source_context['span'],
                                    AdapterProfile(True,package=self._source_package()))
            code=adaptation.generated_text
        job=uuid.uuid4().hex
        folder=self.runtime/job
        # Stage every file before any state changes: a failure here leaves the
        # kernel exactly as it was (idle), with a clear error and no stray folder.
        try:
            folder.mkdir()
            (folder/'code.m').write_text(code)
            if adaptation is not None:
                metadata={**adaptation.metadata(),'original_text':original,'adapted_text':code,
                          'source_context':source_context,'job':job,'epoch':self.generation,'profile':'matlab'}
                (folder/'source.json').write_text(json.dumps(metadata,ensure_ascii=False),encoding='utf-8')
            # The entry function rehashes and applies preapply.txt in its own workspace.
            if apply_breakpoints:(folder/'preapply.txt').write_text(self._breakpoint_commands())
        except OSError as exc:
            shutil.rmtree(folder,ignore_errors=True)
            raise ValueError(tr('Could not prepare the job (disk or permission error): {error}', error=exc)) from exc
        self.job=job
        self.state.update(waiting_input=False,status='starting' if initializing else 'running',job=job,output='',error=None,started=time.time(),elapsed=0,kind=mode,detail=None,workspace_action=None,variable_action=None,breakpoint_relocation=None,console_clear=0,debug=None)
        self._source_job=(job,adaptation,source_context) if adaptation is not None else None
        for name in ('source_adapter','source_error_locations','raw_error','error_frames'):
            self.state.pop(name,None)
        if adaptation is not None:
            # Maps and whole documents stay in private job staging. The UI owns
            # its submitted snapshot and receives bounded transparency metadata.
            self.state['source_adapter']={key:value for key,value in metadata.items()
                if key in ('status','adapter_version','diagnostics','replacements','package_fingerprint','span','job','epoch','profile')}
        proc,events,gen=self.proc,self.events,self.generation
        execute=self.call('__mf_execute__',str(folder),mode,argument)+"\n"
        if apply_breakpoints:
            command=self._path_commands()+' '+execute
        else:
            # Even addpath can refresh Octave's function table. Relocation must
            # enter code.m first while the removed old file is still cached, so
            # dbclear can remove its breakpoint before any path refresh.
            command=f"(@getappdata)(0,'__mf_entry__')({self.quote(folder)},{self.quote(mode)},{self.quote(argument)});\n"
        self._write_command(proc, command)
        threading.Thread(target=self._collect,args=(job,folder,proc,events,gen,float(timeout)),daemon=True).start()
        return job

    def _collect(self,job,folder,proc,events,gen,timeout):
        marker=f'__MF_DONE_{job}__';clear_marker=f'__MF_CLC_{job}__';debug_marker=f'__MF_DEBUG_{job}__';debug_inspect_marker=f'__MF_DEBUG_INSPECT_{job}__';rtc_ready_marker=f'__MF_RTC_READY_{job}__'
        raw=''; clear_count=0; debug_stops=0; debug_prompts=0; debug_serial=0; cleanup_marker=None; deadline=time.monotonic()+timeout if timeout else float('inf'); interrupted=None; reason=None; synced=False; ack=False
        decoder=codecs.getincrementaldecoder('utf-8')(errors='replace')
        while True:
            with self.lock:
                if gen!=self.generation or self.closed: return
                if self.state['status']=='stopping' and interrupted is None:
                    interrupted=time.monotonic(); reason=tr('Operation stopped.')
            if time.monotonic()>deadline and interrupted is None:
                interrupted=time.monotonic(); reason=tr('The {seconds:g}-second time limit was exceeded.', seconds=timeout)
                with self.lock:self.state['status']='stopping'
                try: os.killpg(proc.pid,signal.SIGINT)
                except ProcessLookupError: pass
            with self.lock:pending_debug_continue=self.run_to_cursor_state is not None and self.state['status']=='stopping'
            if interrupted and pending_debug_continue and time.monotonic()-interrupted>.25:
                with self.lock:
                    repeat_interrupt=self.run_to_cursor_state is not None and not self.run_to_cursor_state.get('interrupt_repeated')
                    if repeat_interrupt:self.run_to_cursor_state['interrupt_repeated']=True
                if repeat_interrupt:
                    try:os.killpg(proc.pid,signal.SIGINT)
                    except ProcessLookupError:pass
            if interrupted and not synced and time.monotonic()-interrupted>.25 and (not pending_debug_continue or time.monotonic()-interrupted>2):
                with self.lock:cleanup=self._take_run_to_cursor_cleanup_locked()
                try: self._write_command(proc, f"{cleanup} {self.call('addpath', self.root / 'octave')} {self._marker_command(f'__MF_ACK_{job}__')} {self.call('__mf_execute__', str(folder), 'snapshot', '')}\n")
                except (BrokenPipeError,OSError): pass
                synced=True
            if interrupted and time.monotonic()-interrupted>(30 if ack else 3):
                self._kill(proc)
                reason += tr(' Octave was terminated; reset the session. Variables could not be recovered.')
                break
            try: data=events.get(timeout=.1)
            except queue.Empty: continue
            if data is None:
                reason=reason or tr('The Octave session closed. Reset the session to continue.')
                break
            raw += decoder.decode(data)
            if clear_marker in raw:
                clear_count+=raw.count(clear_marker);raw=raw.rsplit(clear_marker,1)[1]
                with self.lock:self.state['console_clear']=clear_count
            input_marker=f'__MF_INPUT_{job}__'
            if input_marker in raw:
                raw=raw.replace(input_marker,'')
                with self.lock:self.state['waiting_input']=True
            prompts=raw.count('debug> ')
            if prompts>debug_prompts:
                debug_prompts=prompts
                with self.lock:stop_at_debug_prompt=self.state['status']=='stopping' and self.run_to_cursor_state is not None
                if stop_at_debug_prompt:
                    with self.lock:cleanup=self._take_run_to_cursor_cleanup_locked()
                    try:self._write_command(proc, f"{cleanup} {self.call('dbquit')}\n")
                    except (BrokenPipeError,OSError):pass
                    ack=True;synced=True;interrupted=time.monotonic()
            if rtc_ready_marker in raw:
                raw=raw.replace(rtc_ready_marker,'')
                with self.lock:
                    stopping=self.state['status']=='stopping'
                    pending_rtc=self.run_to_cursor_state is not None
                if stopping and pending_rtc:
                    with self.lock:cleanup=self._take_run_to_cursor_cleanup_locked()
                    try:self._write_command(proc, f"{cleanup} {self.call('dbquit')}\n")
                    except (BrokenPipeError,OSError):pass
                    ack=True;interrupted=time.monotonic()
                elif stopping:
                    pass
                elif (folder/'run-to-cursor-error.txt').exists():
                    with self.lock:cleanup=self._take_run_to_cursor_cleanup_locked()
                    try:self._write_command(proc, f"{cleanup} {self.call('__mf_debug_snapshot__', str(folder))}\n")
                    except (BrokenPipeError,OSError):pass
                else:
                    with self.lock:
                        if self.run_to_cursor_state:self.run_to_cursor_state['continued']=True
                    try:self._write_command(proc, self.call('dbcont') + '\n')
                    except (BrokenPipeError,OSError):pass
            stops=list(re.finditer(r'stopped in .*? at line (\d+) \[([^\]\r\n]+)\]',raw))
            if len(stops)>debug_stops:
                with self.lock:
                    navigating=self.debug_frame_target is not None
                debug_stops=len(stops)
                if navigating:
                    try:self._write_command(proc, self.call('__mf_debug_snapshot__', str(folder)) + '\n')
                    except (BrokenPipeError,OSError):pass
                else:
                    match=stops[-1]
                    debug={'file':match.group(2),'line':int(match.group(1)),'ready':False,'serial':debug_serial}
                    with self.lock:
                        stopping=self.state['status']=='stopping'
                        if gen==self.generation and not stopping:self.state.update(status='paused',debug=debug,waiting_input=False)
                        cleanup=self._take_run_to_cursor_cleanup_locked() if stopping else self._run_to_cursor_hit_cleanup_locked(debug['file'],debug['line'])
                    command=f"{cleanup} {self.call('dbquit')}\n" if stopping else f"{cleanup} {self.call('__mf_debug_snapshot__',str(folder))}\n"
                    try:self._write_command(proc, command)
                    except (BrokenPipeError,OSError):pass
            if debug_marker in raw:
                raw=raw.replace(debug_marker,'');debug_serial+=1
                try:debug_meta=json.loads((folder/'debug.json').read_text())
                except (OSError,ValueError):debug_meta={'variables':[]}
                with self.lock:
                    if (folder/'run-to-cursor-error.txt').exists():self.run_to_cursor_state=None
                    if gen==self.generation and self.state.get('debug'):
                        self.state['variables']=debug_meta.get('variables',[])
                        self.state['debug']['stack']=debug_meta.get('stack',[])
                        if self.debug_frame_target is not None:
                            for frame in self.state['debug']['stack']:frame['current']=frame.get('index')==self.debug_frame_target
                            self.debug_frame_target=None
                        current=next((frame for frame in self.state['debug']['stack'] if frame.get('current')),None)
                        if current:self.state['debug'].update(file=current['file'],line=current['line'],frame=current['index'])
                        self.state['debug'].pop('detail',None)
                        self.state['debug'].update(ready=True,serial=debug_serial)
                        self.state['status']='paused'
            if debug_inspect_marker in raw:
                raw=raw.replace(debug_inspect_marker,'');debug_serial+=1
                try:debug_detail=json.loads((folder/'debug-inspect.json').read_text())
                except (OSError,ValueError):debug_detail={'error':tr('Could not retrieve the variable view.')}
                with self.lock:
                    if gen==self.generation and self.state.get('debug'):
                        self.state['debug'].pop('detail',None)
                        if debug_detail.get('error'):self.state['error']=debug_detail['error']
                        else:self.state['debug']['detail']=debug_detail.get('detail')
                        self.state['debug'].update(ready=True,serial=debug_serial)
                        self.state['status']='paused'
            ack_marker=f'__MF_ACK_{job}__'
            if ack_marker in raw:
                raw=raw.replace(ack_marker,'');ack=True;interrupted=time.monotonic()
            if marker in raw:
                raw=raw.split(marker)[0]
                with self.lock:cleanup=self._take_run_to_cursor_cleanup_locked()
                if cleanup:
                    cleanup_marker=f'__MF_RTC_CLEAN_{job}__'
                    try:self._write_command(proc, f'{cleanup} {self._marker_command(cleanup_marker)}\n')
                    except (BrokenPipeError,OSError):break
                else:break
            if cleanup_marker and cleanup_marker in raw:
                raw=raw.replace(cleanup_marker,'')
                break
            if len(raw)>1_000_000: raw=tr('[Previous output truncated]\n')+raw[-800_000:]
            with self.lock:
                if gen==self.generation:
                    self.state['output']=self._clean(raw)
                    self.state['elapsed']=round(time.time()-self.state['started'],2)
        with self.lock:
            if gen!=self.generation or self.closed:return
            self.state['output']=self._clean(raw)
            self.state['elapsed']=round(time.time()-self.state['started'],3)
            try:
                meta=json.loads((folder/'result.json').read_text())
                if 'figures' in meta:
                    for figure in meta['figures']:figure['job']=job
                frames=meta.pop('error_frames',None)
                if isinstance(frames,dict):frames=[frames] if frames else []
                self.state.update(meta)
                source_job=getattr(self,'_source_job',None)
                if source_job and source_job[0]==job:
                    _,adaptation,context=source_job
                    self.state['raw_error']=self.state.get('error')
                    self.state['error_frames']=frames or []
                    # External file frames are native and retain their raw
                    # locations. Only evalin's submitted-source messages map.
                    external=any(frame.get('file') and '__mf_' not in frame.get('name','') for frame in frames or [])
                    if self.state.get('error') and not external:
                        self.state['error'],locations=map_error(adaptation,context,self.state['error'])
                        self.state['source_error_locations']=[{**item,'job':job,'epoch':gen} for item in locations]
            except (OSError,ValueError):
                if not reason: reason=tr('No session response received. Reset the session.')
            self.state['error']=reason or self.state.get('error') or None
            change=self.breakpoint_jobs.pop(job,None)
            if change is not None and change.get('relocation'):
                outcomes=self.state.pop('breakpoint_relocation',None)
                self._finish_relocation(change['request'],outcomes if isinstance(outcomes,list) else [],self.state['error'])
            elif change is not None and self.state['error']:self.breakpoints=change
            self.state['status']='dead' if proc.poll() is not None or not (folder/'result.json').exists() else 'idle'
            self.state['job']=job
            self.state['waiting_input']=False
            self.state['debug']=None
            self.debug_frame_target=None
            self._prune()

    def _clean(self,text):
        import re
        text=re.sub(r'\x1b\[[0-9;]*[A-Za-z]','',text)
        text=re.sub(r'^octave:\d+> ?','',text,flags=re.M)
        text=re.sub(r'__MF_(?:DONE|ACK|INPUT|CLC|DEBUG(?:_INSPECT)?|RTC_CLEAN|RTC_READY)_[a-f0-9]{32}__','',text)
        text=text.replace('debug> ','')
        text=re.sub(r'(?:^|\n)stopped in .*? at line \d+ \[[^\]\r\n]+\]\s*\n\d+:.*?(?=\n|$)','',text)
        text=re.sub(r'(?:^|\n)stopped in .*? at line \d+ \[[^\]\r\n]+\]\s*(?=\n|$)','',text)
        text=re.sub(r'__MF_[A-Za-z0-9_]*$','',text)
        text=re.sub(r'^.*FALLBACK \(log once\).*\n?', '',text,flags=re.M)
        return text.strip('\n')

    def interrupt(self):
        with self.lock:
            if self.state['status']=='paused':
                self.state['status']='stopping';self.state['debug']=None
                cleanup=self._take_run_to_cursor_cleanup_locked()
                try:self._write_command(self.proc, f"{cleanup} {self.call('dbquit')}\n")
                except (BrokenPipeError,OSError):pass
                return
            if self.state['status']=='running' and self.run_to_cursor_state and not self.run_to_cursor_state.get('continued'):
                self.state['status']='stopping';self.state['debug']=None
                cleanup=self._take_run_to_cursor_cleanup_locked()
                try:self._write_command(self.proc, f"{cleanup} {self.call('dbquit')}\n")
                except (BrokenPipeError,OSError):pass
                return
            if self.state['status']=='stopping' and self.run_to_cursor_state and self.run_to_cursor_state.get('continued'):
                try:os.killpg(self.proc.pid,signal.SIGINT)
                except ProcessLookupError:pass
                return
            if self.state['status'] not in ('running','starting'):return
            self.state['status']='stopping'
            try:os.killpg(self.proc.pid,signal.SIGINT)
            except ProcessLookupError:pass

    def input(self,text):
        with self.lock:
            if self.state['status']!='running' or not self.state.get('waiting_input'):raise ValueError(tr('No operation is waiting for input.'))
            self.state['waiting_input']=False
            folder=self.runtime/self.job
            tmp=folder/'input.tmp';tmp.write_text(text,encoding='utf-8');tmp.replace(folder/'input.txt')

    @staticmethod
    def _condition(condition):
        if not isinstance(condition,str) or len(condition)>1000 or any(char in condition for char in ('\x00','\n','\r')):raise ValueError(tr('The condition must be a single line of at most 1000 characters.'))
        condition=condition.strip()
        quote=None;i=0
        while i<len(condition):
            char=condition[i]
            if quote:
                if char=='\\' and quote=='"':i+=2;continue
                if char==quote:
                    if i+1<len(condition) and condition[i+1]==quote:i+=2;continue
                    quote=None
            elif char in "'\"":quote=char
            i+=1
        if quote:raise ValueError(tr('The condition has an unclosed quote.'))
        return condition

    @staticmethod
    def _breakpoint_name(path):
        stem=Path(path).stem
        return bool(re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*',stem)) and stem not in _OCTAVE_KEYWORDS

    def _breakpoint_target(self,path,line):
        path=Path(path).resolve();line=int(line)
        if path.suffix.lower()!='.m' or not path.is_file():raise ValueError(tr('Select a saved .m file for the breakpoint.'))
        if not self._breakpoint_name(path):raise ValueError(tr('The file name must be a valid Octave function name.'))
        if line<1:raise ValueError(tr('Invalid line number.'))
        return path,line

    def _clone_breakpoints(self):
        return {path:{line:dict(item) for line,item in points.items()} for path,points in self.breakpoints.items()}

    def _set_breakpoint_command(self,path,line,condition='',capture=True):
        args=f"{self.quote(Path(path).stem)},{self.quote(line)}"
        if condition:args+=f",'if',{self.quote(condition)}"
        return f"__mf_bp__=dbstop({args}); clear __mf_bp__;" if capture else f"dbstop({args});"

    def _clear_breakpoint_command(self,path,line):
        # Refuses a command-line function name: dbclear on one crashes Octave 11.3.
        return f"__mf_dbclear__({self.quote(Path(path).stem)},{self.quote(line)});"

    def _breakpoint_commands(self):
        commands=[]
        for path,points in sorted(self.breakpoints.items()):
            p=Path(path);name=p.stem
            enabled=[(line,item) for line,item in sorted(points.items()) if item['enabled']]
            if not enabled:continue
            setters=' '.join(self._set_breakpoint_command(path,line,item['condition']) for line,item in enabled)
            failure_format=self.quote(tr('Could not apply breakpoint: %s') + chr(92) + 'n')
            commands.append(f"try; addpath({self.quote(p.parent)},'-begin'); rehash; {setters} catch __mf_bp_err__; fprintf(2,{failure_format},__mf_bp_err__.message); clear __mf_bp_err__; end_try_catch;")
        return ' '.join(commands)

    def breakpoint(self,path,line,enabled,condition=''):
        return self.configure_breakpoint(path,line,'set' if enabled else 'remove',condition,enabled)

    def configure_breakpoint(self,path,line,action,condition=None,enabled=None):
        path,line=self._breakpoint_target(path,line);key=str(path)
        if action not in ('set','enable','disable','remove'):raise ValueError(tr('Invalid breakpoint action.'))
        if enabled is not None and not isinstance(enabled,bool):raise ValueError(tr('Invalid breakpoint state.'))
        if condition is not None:condition=self._condition(condition)
        with self.lock:
            if self.state['status'] in ('running','starting','stopping','paused'):raise ValueError(tr('Finish the operation before changing a breakpoint.'))
            previous=self._clone_breakpoints();points=self.breakpoints.setdefault(key,{})
            current=points.get(line,{'enabled':True,'condition':''})
            code=self._clear_breakpoint_command(path,line)
            if action=='remove':
                points.pop(line,None)
                if not points:self.breakpoints.pop(key,None)
            else:
                item={'enabled':current['enabled'],'condition':current['condition']}
                if action=='enable':item['enabled']=True
                if action=='disable':item['enabled']=False
                if action=='set':item['enabled']=True if enabled is None else bool(enabled)
                if condition is not None:item['condition']=condition
                points[line]=item
                if item['enabled']:code+=f" addpath({self.quote(path.parent)},'-begin'); rehash; {self._set_breakpoint_command(path,line,item['condition'])}"
            job=self._submit(code,'breakpoint','',30);self.breakpoint_jobs[job]=previous;return job

    def clear_breakpoints(self):
        with self.lock:
            if self.state['status'] in ('running','starting','stopping','paused'):raise ValueError(tr('Finish the operation before clearing breakpoints.'))
            previous=self._clone_breakpoints()
            code=' '.join(self._clear_breakpoint_command(path,line) for path,points in previous.items() for line,item in points.items() if item['enabled'])
            self.breakpoints={}
            job=self._submit(code,'breakpoint','',30);self.breakpoint_jobs[job]=previous;return job

    def _take_run_to_cursor_cleanup_locked(self):
        pending=self.run_to_cursor_state
        if not pending:return ''
        self.run_to_cursor_state=None
        original=pending.get('original')
        restore=bool(original and original['enabled'])
        condition=original['condition'] if restore else ''
        command=self.call('__mf_run_to_cursor_cleanup__',str(pending['folder']),pending['file'],str(pending['line']),int(restore),condition)
        return command

    def _run_to_cursor_hit_cleanup_locked(self,path,line):
        pending=self.run_to_cursor_state
        if not pending:return ''
        try:same_file=Path(path).resolve()==Path(pending['file'])
        except OSError:same_file=False
        actual=pending.get('actual_line')
        actual_file=self.runtime/self.job/'run-to-cursor.txt'
        if actual is None:
            try:actual=int(actual_file.read_text().splitlines()[0])
            except (OSError,ValueError):actual=pending['line']
            pending['actual_line']=actual
        return self._take_run_to_cursor_cleanup_locked() if same_file and line==actual else ''

    def run_to_cursor(self,path,line):
        path,line=self._breakpoint_target(path,line);key=str(path)
        with self.lock:
            debug=self.state.get('debug')
            if self.state['status']!='paused' or not debug or not debug.get('ready'):raise ValueError(tr('Run to Cursor is available only at a real debugger pause.'))
            if self.run_to_cursor_state:raise ValueError(tr('Run to Cursor is already pending.'))
            original=self.breakpoints.get(key,{}).get(line)
            folder=self.runtime/self.job
            (folder/'run-to-cursor.txt').unlink(missing_ok=True)
            (folder/'run-to-cursor-error.txt').unlink(missing_ok=True)
            command=self.call('__mf_run_to_cursor__',str(folder),str(path),str(line))+'\n'
            self.run_to_cursor_state={'file':key,'line':line,'actual_line':None,'original':dict(original) if original else None,'job':self.job,'folder':str(folder),'continued':False,'interrupt_repeated':False}
            self.state['status']='running';self.state['debug']['ready']=False
            try:self._write_command(self.proc, command)
            except (BrokenPipeError,OSError):
                self.run_to_cursor_state=None
                raise ValueError(tr('Octave closed. Reset the session.'))

    def relocate_file_breakpoints(self,source,destination):
        """File-operation hook, called after the filesystem move already succeeded.

        The canonical map is reconciled first and only ever describes existing
        .m targets: breakpoints of a target that is gone or is not a valid
        breakpoint target are dropped. Then one identified job clears the old
        Octave locations and installs the moved enabled ones, one operation each.
        The per-breakpoint outcome is delivered as state.breakpoint_relocation
        ({kind, moved, dropped, failed} rows with source/path/line/message).
        """
        source=str(Path(source).resolve())
        destination=str(Path(destination).resolve()) if destination is not None else None
        with self.lock:
            busy=self.state['status']!='idle' or self.state.get('waiting_input')
            affected=[path for path in self.breakpoints if path==source or path.startswith(source+'/')]
            if not affected:
                if busy:raise ValueError(tr('Finish the Octave operation before relocating breakpoints.'))
                assert self.run_to_cursor_state is None, tr('An idle engine cannot have a temporary breakpoint.')
                return None
            ops=[];moved=[];dropped=[];installs=[]
            for path in affected:
                points=self.breakpoints.pop(path)
                target=destination+path[len(source):] if destination is not None else None
                valid=target is not None and Path(target).suffix.lower()=='.m' and Path(target).is_file() and self._breakpoint_name(target)
                enabled=sorted(line for line,item in points.items() if item['enabled'])
                if enabled:
                    resolver=target if valid else str(self.root/'octave'/'__mf_execute__.m')
                    ops.append({'id':f'clear:{path}','code':f"__mf_breakpoint_clear_moved__({self.quote(Path(path).stem)},[{' '.join(map(str,enabled))}],{self.quote(resolver)});"})
                for line,item in sorted(points.items()):
                    row={'source':path,'path':target or path,'line':line,'enabled':item['enabled'],'condition':item['condition']}
                    if not valid:dropped.append({**row,'message':tr('The breakpoint target is no longer a valid .m file.')})
                    elif not item['enabled']:moved.append(row)
                    else:
                        installs.append(row)
                        ops.append({'id':f'set:{path}:{line}','requires':f'clear:{path}','code':f"addpath({self.quote(Path(target).parent)},'-begin'); rehash; {self._set_breakpoint_command(target,line,item['condition'])}"})
                if valid:self.breakpoints.setdefault(target,{}).update({line:dict(item) for line,item in points.items()})
            request={'moved':moved,'dropped':dropped,'installs':installs}
            def forget(rows):
                for row in rows:
                    points=self.breakpoints.get(row['path'],{});points.pop(row['line'],None)
                    if not points:self.breakpoints.pop(row['path'],None)
            try:
                if busy:raise ValueError(tr('Finish the Octave operation before relocating breakpoints.'))
                assert self.run_to_cursor_state is None, tr('An idle engine cannot have a temporary breakpoint.')
                # code.m stays empty: operations travel as typed JSON and run in
                # Octave's function workspace, each in its own try block.
                job=self._submit('','breakpoint',json.dumps({'ops':ops},ensure_ascii=False),30,apply_breakpoints=False)
            except Exception as exc:
                # The new enabled locations were not installed; never resurrect
                # the removed source paths after the filesystem move succeeded.
                forget(installs)
                self.state['breakpoint_relocation']={'kind':'relocate','moved':moved,'dropped':dropped,'failed':[{**row,'message':tr('Could not submit the breakpoint job: {error}', error=str(exc))} for row in installs]}
                raise
            self.breakpoint_jobs[job]={'relocation':True,'request':request}
            return job

    def _finish_relocation(self,request,outcomes,error):
        """Build the per-breakpoint report after the relocation job. Caller holds the lock."""
        outcomes={item.get('id'):item for item in outcomes or []}
        moved=list(request['moved']);failed=[]
        for row in request['installs']:
            clear=outcomes.get(f"clear:{row['source']}");install=outcomes.get(f"set:{row['source']}:{row['line']}")
            if clear is not None and not clear.get('ok'):message=tr('Could not clear the previous breakpoint: {error}', error=str(clear.get('message')))
            elif install is None:message=error or tr('No breakpoint job result received.')
            elif not install.get('ok'):message=str(install.get('message'))
            else:message=None
            if message is None:moved.append(row)
            else:failed.append({**row,'message':message})
        for row in failed:
            points=self.breakpoints.get(row['path'],{});points.pop(row['line'],None)
            if not points:self.breakpoints.pop(row['path'],None)
        # A dropped row whose old Octave location could not be cleared is still
        # dropped from the map; say that the engine may still hold it.
        dropped=[]
        for row in request['dropped']:
            clear=outcomes.get(f"clear:{row['source']}")
            if clear is not None and not clear.get('ok'):row={**row,'message':tr('{message} Could not clear the old breakpoint in Octave: {error}', message=row['message'], error=str(clear.get('message')))}
            dropped.append(row)
        report={'kind':'relocate','moved':moved,'dropped':dropped,'failed':failed}
        self.state['breakpoint_relocation']=report

    def debug(self,command,code=''):
        controls={'continue':self.call('dbcont'),'step':self.call('dbstep'),'in':self.call('dbstep','in'),'out':self.call('dbstep','out'),'quit':self.call('dbquit')}
        with self.lock:
            debug=self.state.get('debug')
            if self.state['status']!='paused' or not debug or not debug.get('ready'):raise ValueError(tr('The debugger is not waiting for a command.'))
            folder=self.runtime/self.job
            if command=='eval':
                if not isinstance(code,str) or not code.strip() or len(code)>100_000 or '\n' in code or '\r' in code:raise ValueError(tr('Enter a single-line Octave expression.'))
                # Prompt-level eval via a handle literal: no helper frame, and return/dbcont keep their meaning.
                # A pending `return` skips the rest of the line, so the snapshot sits in unwind_protect_cleanup.
                line=f"unwind_protect; (@eval)({self.quote(code)},'(@__mf_debug_report__)();'); unwind_protect_cleanup; {self.call('__mf_debug_snapshot__',str(folder))} end_unwind_protect\n"
            elif command=='inspect':
                if not isinstance(code,str) or len(code)>63 or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*',code) or code.startswith('__mf_'):raise ValueError(tr('Invalid or reserved variable name.'))
                line=self.call('__mf_debug_inspect__',str(folder),code)+'\n'
            elif command=='frame':
                try:target=int(code)
                except (TypeError,ValueError):raise ValueError(tr('Invalid call stack frame.'))
                stack=debug.get('stack') or [];current=next((frame for frame in stack if frame.get('current')),None);selected=next((frame for frame in stack if frame.get('index')==target),None)
                if not current or not selected:raise ValueError(tr('The call stack frame is no longer valid.'))
                delta=target-current['index']
                if delta==0:return
                move=self.call('dbup',delta) if delta>0 else self.call('dbdown',-delta)
                line=move+'\n'
                self.debug_frame_target=target
            elif command=='quit':line=f"{self._take_run_to_cursor_cleanup_locked()} {self.call('dbquit')}\n{self.call('addpath',self.root/'octave')} {self.call('__mf_execute__',str(folder),'snapshot','')}\n"
            elif command in controls:line=controls[command]+'\n'
            else:raise ValueError(tr('Invalid debugger command.'))
            self.state['status']='running';self.state['debug']['ready']=False
            try:self._write_command(self.proc, line)
            except (BrokenPipeError,OSError):
                self.debug_frame_target=None
                raise ValueError(tr('Octave closed. Reset the session.'))

    def reset(self):
        with self.lock:
            self.generation+=1
            self._kill(self.proc)
            self.breakpoint_jobs.clear()
            self.run_to_cursor_state=None
            self.debug_frame_target=None
            self.state.update(status='starting',variables=[],figures=[],error=None,output='',waiting_input=False,console_clear=0,debug=None)
            try:self.start()
            except Exception as exc:
                self.state.update(status='dead',error=tr('Could not start Octave: {error}', error=exc))
                raise

    def _prune(self):
        protected={self.state.get('job')}|{f.get('job') for f in self.state.get('figures',[])}
        folders=sorted((p for p in self.runtime.iterdir() if p.is_dir()),key=lambda p:p.stat().st_mtime,reverse=True)
        for folder in folders[20:]:
            if folder.name not in protected:shutil.rmtree(folder,ignore_errors=True)

    def _kill(self,proc):
        if not proc:return
        try:os.killpg(proc.pid,signal.SIGTERM)
        except ProcessLookupError:pass
        try:proc.wait(timeout=1)
        except subprocess.TimeoutExpired:
            try:os.killpg(proc.pid,signal.SIGKILL)
            except ProcessLookupError:pass
            proc.wait(timeout=2)
        for pipe in (proc.stdin,proc.stdout):
            try:pipe.close()
            except (OSError,AttributeError):pass

    def close(self):
        with self.lock:
            self.closed=True;self.generation+=1;self._kill(self.proc)
