"""Rebuild project-local Octave package index after moving this directory."""
from pathlib import Path
import shutil,subprocess,os
root=Path(__file__).resolve().parents[1]
p=root/'.packages';p.mkdir(exist_ok=True)
quote=lambda s:"'"+str(s).replace("'","''")+"'"
exe=os.environ.get('MATLAB_FREE_OCTAVE') or shutil.which('octave') or '/opt/homebrew/bin/octave'
code=f"pkg('prefix',{quote(p)});pkg('local_list',{quote(p/'octave_packages')});pkg('rebuild');pkg('list');"
subprocess.run([exe,'--no-gui','--quiet','--eval',code],check=True,cwd=root)
