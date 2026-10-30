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
                    *(str(ROOT / 'macos' / name) for name in ('App.swift', 'Splash.swift', 'main.swift')),
                    '-o', str(executable),
                    '-framework', 'Cocoa', '-framework', 'WebKit'], check=True)
    shutil.copy2(ROOT / 'macos' / 'launch.py', resources / 'launch.py')
    (resources / 'launcher.json').write_text(json.dumps({'project_root': str(ROOT), 'python': PYTHON}, indent=2))
    # The launch screen's logo and type, from the brand kit. Splash.swift names the same files.
    splash = resources / 'splash'
    splash.mkdir()
    for variant in ('black', 'white'):
        shutil.copy2(ROOT / 'docs' / 'brand' / 'logo' / 'png' / f'indymat-logo-horizontal-{variant}-1024.png', splash)
    for subset in ('latin', 'latin-ext'):
        for weight in (400, 600):
            shutil.copy2(ROOT / 'docs' / 'brand' / 'fonts' / 'web' / f'montserrat-{subset}-{weight}-normal.woff2', splash)
    shutil.copy2(ROOT / 'docs' / 'brand' / 'fonts' / 'OFL.txt', splash / 'montserrat-OFL.txt')
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
    # macOS 26 icon with light and dark appearances (Icon Composer document); the .icns stays as the
    # fallback for older systems. actool writes Assets.car, CFBundleIconName selects the icon in it.
    plist = temp / 'icon-partial.plist'
    subprocess.run(['/usr/bin/xcrun', 'actool', '--compile', str(resources), '--platform', 'macosx',
                    '--minimum-deployment-target', '12.0', '--app-icon', 'IndyMAT',
                    '--output-partial-info-plist', str(plist), str(ROOT / 'macos' / 'IndyMAT.icon')],
                   check=True, stdout=subprocess.DEVNULL)
    info = dict(CFBundleName='IndyMAT', CFBundleDisplayName='IndyMAT',
                CFBundleIdentifier='local.indymat.launcher',
                CFBundleExecutable='IndyMAT', CFBundlePackageType='APPL',
                CFBundleIconFile='IndyMAT.icns', CFBundleIconName='IndyMAT',
                CFBundleShortVersionString='0.1.1', CFBundleVersion='2',
                LSMinimumSystemVersion='12.0', NSHighResolutionCapable=True,
                NSPrincipalClass='NSApplication',
                NSAppTransportSecurity={'NSAllowsLocalNetworking': True},
                NSHumanReadableCopyright='IndyMAT contributors')
    with (bundle / 'Contents' / 'Info.plist').open('wb') as f:
        plistlib.dump(info, f)
    subprocess.run(['/usr/bin/codesign', '--force', '--sign', '-', str(bundle)], check=True)
    # A fresh copy: a file the previous build had and this one lacks would break the signature.
    shutil.rmtree(APP, ignore_errors=True)
    shutil.copytree(bundle, APP)
subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(APP)], check=True)
print(f'Built and verified: {APP}')
