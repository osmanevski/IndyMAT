"""Private control fixture using the complete manifest and project registry."""
import contextlib
import io
from pathlib import Path
import shutil
import subprocess

from scripts import patch_packages


def prepare_control(root, project, octave):
    """Check/apply the full patch set; never discard creation ownership."""
    package = project / '.packages' / 'control-4.2.3'
    shutil.copytree(root / '.packages' / 'control-4.2.3', package)
    patches = root / 'octave' / 'paket-yamalari' / 'control-4.2.3'
    with contextlib.redirect_stdout(io.StringIO()):
        states = patch_packages.patch_package(project, patches, check=True)
        if any(state == 'modified' for state in states.values()):
            raise patch_packages.PatchError('Modified control fixture; refusing to apply patches')
        patch_packages.patch_package(project, patches)
    # The application loads this registry. Respect its preserved archprefix,
    # without assuming a HOME directory, API version, or architecture.
    registry = root / '.packages' / 'octave_packages'
    quote = lambda value: "'" + str(value).replace("'", "''") + "'"
    code = (f"data=load({quote(registry)}); found=false; "
            "for k=1:numel(data.local_packages), p=data.local_packages{k}; "
            "if strcmp(p.name,'control') && strcmp(p.version,'4.2.3'), "
            "arch=[__octave_config_info__('canonical_host_type') '-' "
            "__octave_config_info__('api_version')]; "
            "fprintf('@@CONTROL_ARCH=%s\\n',fullfile(p.archprefix,arch)); found=true; end; end; "
            "assert(found,'Control 4.2.3 missing from project package registry');")
    result = subprocess.run([octave, '--quiet', '--no-init-file', '--eval', code],
                            capture_output=True, text=True, check=True, timeout=30)
    paths = [line.removeprefix('@@CONTROL_ARCH=') for line in result.stdout.splitlines()
             if line.startswith('@@CONTROL_ARCH=')]
    if len(paths) != 1:
        raise RuntimeError('Expected one control architecture path in project registry')
    arch = Path(paths[0])
    if not arch.is_dir() or not any(arch.glob('*.oct')):
        raise RuntimeError(f'Control binaries missing at registered architecture path: {arch}')
    return package, arch
