"""Install a verified build, retaining the previous package as a temporary backup."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[1]
source = ROOT / '.build.noindex' / 'IndyMAT.app'
target = Path('/Applications/IndyMAT.app')
register = '/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister'
subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(source)], check=True)
with (source / 'Contents/Info.plist').open('rb') as f:
    assert plistlib.load(f)['CFBundleExecutable'] == 'IndyMAT'
staging = target.parent / ('.IndyMAT-install-' + uuid.uuid4().hex + '.app')
subprocess.run(['/usr/bin/ditto', str(source), str(staging)], check=True)
subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(staging)], check=True)
backup = Path(tempfile.mkdtemp(prefix='indymat-previous-'))
if target.exists():
    subprocess.run([register, '-u', str(target)], check=False, capture_output=True)
    shutil.move(str(target), str(backup / 'IndyMAT.previous-bundle'))
try:
    staging.rename(target)
except Exception:
    previous = backup / 'IndyMAT.previous-bundle'
    if previous.exists():shutil.move(str(previous), str(target))
    raise
legacy = ROOT / 'build' / 'IndyMAT.app'
if legacy.exists():
    subprocess.run([register, '-u', str(legacy)], check=False, capture_output=True)
    shutil.move(str(legacy), str(backup / 'IndyMAT.build-copy-bundle'))
subprocess.run([register, '-f', str(target)], check=True, capture_output=True)
subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(target)], check=True)
print(f'Installed: {target}')
print(f'Previous packages preserved: {backup}')
subprocess.run(['/usr/bin/open', str(target)], check=True)
