"""Short-lived, parse-only GNU Octave syntax checks."""
from __future__ import annotations
from backend.i18n import tr
import os, re, subprocess, tempfile, threading
from pathlib import Path
from backend.kernel import cli_executable

class LintTimeout(TimeoutError):pass

class OctaveLinter:
    MAX_BYTES=500_000
    def __init__(self,runtime,executable,timeout=5.0):
        self.runtime=Path(runtime);self.runtime.mkdir(parents=True,exist_ok=True);os.chmod(self.runtime,0o700)
        self.executable=cli_executable(executable);self.timeout=float(timeout);self.lock=threading.Lock()
    def check(self,code):
        if not isinstance(code,str):raise ValueError(tr('The code must be text.'))
        if len(code.encode('utf-8'))>self.MAX_BYTES:raise ValueError(tr('The syntax check limit is 500 KB.'))
        with self.lock:
            handle=tempfile.NamedTemporaryFile(mode='w',encoding='utf-8',suffix='.m',prefix='buffer-',dir=self.runtime,delete=False)
            source=Path(handle.name)
            try:
                with handle:handle.write(code)
                os.chmod(source,0o600)
                env=os.environ.copy();env['INDYMAT_LINT_FILE']=str(source)
                try:
                    result=subprocess.run([self.executable,'--no-gui','--quiet','--no-init-file','--no-site-file','--no-history','--eval',"builtin('__parse_file__', getenv('INDYMAT_LINT_FILE'), false);"],cwd=self.runtime,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,encoding='utf-8',errors='replace',timeout=self.timeout,check=False)
                except subprocess.TimeoutExpired as exc:raise LintTimeout(tr('The syntax check timed out.')) from exc
                if result.returncode==0:return []
                match=re.search(r'(?:parse|syntax) error near line\s+(\d+)(?:,\s*column\s+(\d+))?',result.stdout,re.I)
                if not match:raise RuntimeError(tr('Octave could not parse the code.'))
                issue={'line':int(match.group(1)),'message':tr('Syntax error.'),'severity':'error'}
                if match.group(2) and int(match.group(2))>0:issue['column']=int(match.group(2))
                return [issue]
            finally:source.unlink(missing_ok=True)
