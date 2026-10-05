"""Apply versioned IndyMAT package patches with exact hashes and no external tools.

Only the selected project's .packages/<name>-<version> tree is written. All
inputs are validated before any writes; replacements are atomic and a failed
commit is rolled back. Run while that private package copy is not being edited.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PATCH_ROOT = ROOT / 'octave' / 'paket-yamalari'
STATE = '.indymat-patches'


class PatchError(ValueError):
    """A package or patch does not match the registered version."""


def digest(data):
    return hashlib.sha256(data).hexdigest()


def relative(value):
    if not isinstance(value, str) or not value:
        raise PatchError('Empty or invalid relative path')
    path = PurePosixPath(value)
    if path.is_absolute() or '..' in path.parts or str(path) != value or '\\' in value:
        raise PatchError(f'Unsafe relative path: {value}')
    return path


def inside(base, name):
    """Reject symlinks, including intermediate directories, before reading/writing."""
    parts = relative(name).parts
    path = base
    if base.is_symlink() or not base.is_dir():
        raise PatchError(f'Expected a real directory: {base}')
    for part in parts:
        path = path / part
        if path.is_symlink():
            raise PatchError(f'Refusing symlink: {path}')
        if path.exists() and path != base.joinpath(*parts) and not path.is_dir():
            raise PatchError(f'Expected a directory: {path}')
    if path.exists() and not path.is_file():
        raise PatchError(f'Expected a regular file: {path}')
    return path


def unified(data, patch):
    """Apply exact unified hunks (no fuzz, offsets, shell or newline conversion)."""
    source = data.decode('utf-8').splitlines(keepends=True)
    lines = patch.splitlines(keepends=True)
    if len(lines) < 3 or not lines[0].startswith('--- a/') or not lines[1].startswith('+++ b/'):
        raise PatchError('Invalid unified diff headers')
    oldname = lines[0][6:].rstrip('\n')
    newname = lines[1][6:].rstrip('\n')
    if oldname != newname:
        raise PatchError('Patches may not rename files')
    relative(oldname)
    out = []
    cursor = 0
    i = 2
    while i < len(lines):
        match = re.fullmatch(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@[^\n]*\n', lines[i])
        if not match:
            raise PatchError('Invalid unified hunk header')
        start, oldcount, newstart, newcount = (int(match[1]), int(match[2] or 1),
                                              int(match[3]), int(match[4] or 1))
        offset = start - 1 if oldcount else start
        if offset < cursor or offset > len(source):
            raise PatchError('Overlapping or out-of-range unified hunk')
        out.extend(source[cursor:offset])
        cursor = offset
        if (newstart - 1 if newcount else newstart) != len(out):
            raise PatchError('Incorrect new-file hunk offset')
        consumed = added = 0
        i += 1
        while i < len(lines) and not lines[i].startswith('@@ '):
            line = lines[i]
            if line[:1] not in (' ', '+', '-'):
                raise PatchError('Unsupported unified diff line')
            body = line[1:]
            if line[0] in ' -':
                if cursor >= len(source) or source[cursor] != body:
                    raise PatchError('Unified hunk context does not match exactly')
                cursor += 1
                consumed += 1
            if line[0] in ' +':
                out.append(body)
                added += 1
            i += 1
        if (consumed, added) != (oldcount, newcount):
            raise PatchError('Unified hunk line counts do not match')
    out.extend(source[cursor:])
    return oldname, ''.join(out).encode('utf-8')


def load_manifest(folder):
    manifest = json.loads((folder / 'manifest.json').read_text(encoding='utf-8'))
    if manifest.get('schema') != 1 or manifest.get('package') != folder.name:
        raise PatchError('Unsupported or mismatched manifest')
    if not re.fullmatch(r'[a-z][a-z0-9_-]*-\d+\.\d+\.\d+', folder.name):
        raise PatchError('Invalid package/version')
    files = manifest.get('files', [])
    if not files or len({f['path'] for f in files}) != len(files):
        raise PatchError('Empty or duplicate target list')
    for item in files:
        relative(item['path'])
        if relative(item['path']).parts[0] == STATE:
            raise PatchError('Patch targets may not name the registry')
        for key in ('pristine_sha256', 'patched_sha256'):
            if not re.fullmatch(r'[a-f0-9]{64}', item[key]):
                raise PatchError('Invalid SHA-256 in manifest')
    if not manifest.get('patches'):
        raise PatchError('No patches in manifest')
    for name in manifest['patches']:
        relative(name)
    return manifest


def atomic(path, data, mode):
    fd, tmp = tempfile.mkstemp(prefix='.indymat-write-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
            os.fchmod(stream.fileno(), mode)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def transaction(package, changes, expected):
    """Preserve originals/modes and roll back write errors, including metadata."""
    originals = {}
    created = []
    # Close the gap between planning and committing; unknown bytes never write.
    for name, data in expected.items():
        path = inside(package, name)
        now = path.read_bytes() if path.exists() else None
        if now != data:
            raise PatchError(f'File changed during patch preparation: {path}; nothing changed')
    try:
        for name in changes:
            path = inside(package, name)
            originals[name] = (path.read_bytes(), stat.S_IMODE(path.stat().st_mode)) if path.exists() else None
        for name, data in changes.items():
            path = inside(package, name)
            missing = []
            parent = path.parent
            while not parent.exists():
                missing.append(parent)
                parent = parent.parent
            for parent in reversed(missing):
                parent.mkdir()
                created.append(parent)
            inside(package, name)
            mode = originals[name][1] if originals[name] else 0o600
            atomic(path, data, mode)
    except BaseException:
        for name, previous in reversed(list(originals.items())):
            path = inside(package, name)
            if previous is None:
                if path.exists():
                    path.unlink()
            elif path.exists() and path.read_bytes() == previous[0]:
                continue
            else:
                atomic(path, *previous)
        for parent in reversed(created):
            parent.rmdir()
        raise


def patch_package(project, folder, *, check=False, revert=False):
    manifest = load_manifest(folder)
    project = Path(project).resolve(strict=True)
    packages = project / '.packages'
    if packages.is_symlink() or not packages.is_dir():
        raise PatchError(f'Expected a real .packages directory: {packages}')
    package = packages / manifest['package']
    if package.is_symlink() or not package.is_dir():
        raise PatchError(f'Package version is absent or symlinked: {package}')
    registry_name = STATE + '/registry.json'
    registry = inside(package, registry_name)
    registry_data = (json.dumps(manifest, indent=2) + '\n').encode('utf-8')
    if registry.exists() and registry.read_bytes() != registry_data:
        raise PatchError(f'Registry does not match this patch set: {registry}')

    # Validate EVERY target and saved original before preparing a single write.
    expected = {registry_name: registry.read_bytes() if registry.exists() else None}
    states = {}
    pristine = {}
    changes = {}
    for item in manifest['files']:
        name = item['path']
        target = inside(package, name)
        if not target.exists():
            raise PatchError(f'Missing target: {target}')
        data = target.read_bytes()
        expected[name] = data
        sha = digest(data)
        if sha == item['pristine_sha256']:
            states[name] = 'pristine'
            pristine[name] = data
        elif sha == item['patched_sha256']:
            states[name] = 'patched'
        else:
            raise PatchError(f'Refusing modified or unsupported target: {target} (SHA-256 {sha}); nothing changed')
        backup_name = STATE + '/pristine/' + name
        backup = inside(package, backup_name)
        expected[backup_name] = backup.read_bytes() if backup.exists() else None
        if backup.exists():
            saved = backup.read_bytes()
            if digest(saved) != item['pristine_sha256']:
                raise PatchError(f'Refusing modified pristine backup: {backup}; nothing changed')
            pristine[name] = saved
        elif states[name] == 'patched' and revert:
            raise PatchError(f'Patched target has no verified pristine backup: {backup}; nothing changed')
        if revert and states[name] == 'patched':
            changes[name] = pristine[name]
        elif not revert and states[name] == 'pristine':
            changes[backup_name] = data

    # Reconstruct and verify all patched bytes from the exact saved baseline.
    patched = dict(pristine)
    for name in manifest['patches']:
        patch = inside(folder, name).read_text(encoding='utf-8')
        target_name = patch.splitlines()[0][6:]
        if target_name not in states:
            raise PatchError(f'Unregistered patch target: {target_name}')
        if target_name in patched:
            target_name, result = unified(patched[target_name], patch)
            patched[target_name] = result
    for item in manifest['files']:
        name = item['path']
        if name in patched and digest(patched[name]) != item['patched_sha256']:
            raise PatchError(f'Patched hash does not match manifest: {name}; nothing changed')
        if not revert and states[name] == 'pristine':
            changes[name] = patched[name]
    if not check and changes:
        changes[registry_name] = registry_data
        transaction(package, changes, expected)
    for name, state in states.items():
        action = 'check' if check else ('reverted' if revert and state == 'patched' else
                 'applied' if not revert and state == 'pristine' else 'skipped')
        print(f"{manifest['package']}/{name}: {state} ({action})")
    return states


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--project', type=Path, default=ROOT, help='project containing .packages (default: this project)')
    parser.add_argument('--package', default='datatypes-1.5.0', choices=[p.name for p in PATCH_ROOT.iterdir() if (p / 'manifest.json').is_file()])
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true', help='report exact hashes without writing')
    mode.add_argument('--revert', action='store_true', help='restore verified saved pristine files')
    args = parser.parse_args(argv)
    try:
        patch_package(args.project, PATCH_ROOT / args.package, check=args.check, revert=args.revert)
    except (PatchError, OSError, ValueError, KeyError, IndexError) as exc:
        print(f'Package patch refused: {exc}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
