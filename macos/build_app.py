"""Build the native IndyMAT window without stopping its local Octave session."""
from pathlib import Path
import json
import plistlib
import platform
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / '.build.noindex' / 'IndyMAT.app'
PYTHON = str(Path(sys.executable).resolve())
APP.parent.mkdir(exist_ok=True)

with tempfile.TemporaryDirectory(prefix='indymat-native-') as temp:
    temp = Path(temp)
    bundle = temp / 'IndyMAT.app'
    resources = bundle / 'Contents' / 'Resources'
    executable = bundle / 'Contents' / 'MacOS' / 'IndyMAT'
    resources.mkdir(parents=True)
    executable.parent.mkdir()
    arch = 'arm64' if platform.machine() == 'arm64' else 'x86_64'
    subprocess.run(['/usr/bin/swiftc', '-swift-version', '5', '-O',
                    '-target', f'{arch}-apple-macos12.0',
                    '-module-cache-path', str(ROOT / '.build.noindex' / 'ModuleCache'),
                    str(ROOT / 'macos' / 'App.swift'), '-o', str(executable),
                    '-framework', 'Cocoa', '-framework', 'WebKit'], check=True)
    shutil.copy2(ROOT / 'macos' / 'launch.py', resources / 'launch.py')
    (resources / 'launcher.json').write_text(json.dumps({'project_root': str(ROOT), 'python': PYTHON}, indent=2))
    icons = temp / 'IndyMAT.iconset'
    icons.mkdir()
    for size in (16, 32, 128, 256, 512):
        for factor in (1, 2):
            filename = f'icon_{size}x{size}' + ('@2x' if factor == 2 else '') + '.png'
            subprocess.run(['/usr/bin/sips', '-z', str(size * factor), str(size * factor),
                            str(ROOT / 'macos' / 'icon.png'), '--out', str(icons / filename)],
                           check=True, stdout=subprocess.DEVNULL)
    subprocess.run(['/usr/bin/iconutil', '-c', 'icns', str(icons),
                    '-o', str(resources / 'IndyMAT.icns')], check=True)
    info = dict(CFBundleName='IndyMAT', CFBundleDisplayName='IndyMAT',
                CFBundleIdentifier='local.indymat.launcher',
                CFBundleExecutable='IndyMAT', CFBundlePackageType='APPL',
                CFBundleIconFile='IndyMAT.icns',
                CFBundleShortVersionString='0.1.1', CFBundleVersion='2',
                LSMinimumSystemVersion='12.0', NSHighResolutionCapable=True,
                NSPrincipalClass='NSApplication',
                NSAppTransportSecurity={'NSAllowsLocalNetworking': True},
                NSHumanReadableCopyright='IndyMAT contributors')
    with (bundle / 'Contents' / 'Info.plist').open('wb') as f:
        plistlib.dump(info, f)
    subprocess.run(['/usr/bin/codesign', '--force', '--sign', '-', str(bundle)], check=True)
    shutil.copytree(bundle, APP, dirs_exist_ok=True)
subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(APP)], check=True)
print(f'Built and verified: {APP}')
