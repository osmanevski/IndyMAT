"""Opt-in saved entry scripts; engine reflection exposes the physical execution file."""
from dataclasses import dataclass, replace, field
from pathlib import Path
from contextlib import contextmanager
import hashlib
import os
import re
import stat
from backend.i18n import tr
from backend.source_adapter import adapt_source, AdapterProfile, SourceSpan
from backend.source_lexer import Diagnostic, MAX_BYTES, lex_source

_CAPABILITY = object()

MAX_FILE_BYTES = 2_000_000
MAX_RETAINED_FILES = 64
MAX_RETAINED_BYTES = 32_000_000

@dataclass(frozen=True)
class SourceFile:
    path: str
    document: str
    sha256: str
    saved_bytes: bytes
    _capability: object = field(default=None, repr=False, compare=False)

    def context(self):
        return {'origin': 'entry-file', 'path': self.path, 'document': self.document,
                'profile': 'matlab', 'cursor': 0, 'revision': self.sha256,
                'snapshot_sha256': self.sha256,
                'span': {'start_utf16': 0, 'end_utf16': len(self.document.encode('utf-16-le')) // 2}}

@contextmanager
def _parent(path):
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    chain = []
    try:
        for part in path.parts[1:-1]:
            nxt = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = nxt
            info = os.fstat(fd)
            chain.append((info.st_dev, info.st_ino))
        yield fd, tuple(chain)
    finally:
        os.close(fd)

def read_file_source(workspace, name, expected):
    if not isinstance(expected, str) or re.fullmatch(r'[0-9a-f]{64}', expected) is None:
        raise ValueError(tr('A saved file hash is required for file adaptation.'))
    path = workspace.path(name)
    if path.suffix.lower() != '.m':
        raise ValueError(tr('Select a saved .m file.'))
    with _parent(path) as (parent, chain):
        descriptor = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
        with os.fdopen(descriptor, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):
                raise ValueError(tr('Select a saved .m file.'))
            if before.st_size > MAX_FILE_BYTES:
                raise ValueError(tr('The Editor limit is 2 MB. Read the file from the Command Window.'))
            data = stream.read(MAX_FILE_BYTES + 1)
            after = os.fstat(stream.fileno())
        if len(data) > MAX_FILE_BYTES:
            raise ValueError(tr('The Editor limit is 2 MB. Read the file from the Command Window.'))
        signature = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
        if signature(before) != signature(after) or hashlib.sha256(data).hexdigest() != expected:
            raise FileExistsError(tr('The saved source changed. Reload or save it before running.'))
        if workspace.path(name) != path:
            raise FileExistsError(tr('The saved source changed. Reload or save it before running.'))
        with _parent(path) as (check, check_chain):
            current = os.stat(path.name, dir_fd=check, follow_symlinks=False)
            if check_chain != chain or signature(current) != signature(after):
                raise FileExistsError(tr('The saved source changed. Reload or save it before running.'))
    try:
        document = data.decode('utf-8-sig')
    except UnicodeDecodeError:
        raise ValueError(tr('The file is not UTF-8. Save it as UTF-8 in its editor and import it again.'))
    return SourceFile(str(path), document, expected, data, _CAPABILITY)

def file_fallback(source, code, message):
    empty = adapt_source('', profile=AdapterProfile())
    return replace(empty, generated_text=source.document, status='fallback',
                   span=SourceSpan(0, len(source.document.encode('utf-16-le')) // 2),
                   diagnostics=(Diagnostic(code, message, 0, 0),))

def adapt_file_source(source, profile, *, debugging=False):
    if not isinstance(source, SourceFile) or source._capability is not _CAPABILITY:
        raise ValueError('Invalid server file source.')
    if debugging:
        return file_fallback(source, 'file-debug', 'File adaptation is unavailable while breakpoints or debugging are active.')
    path = Path(source.path)
    parts=path.parts[1:]
    # macOS /private is a filesystem root, not an Octave private hierarchy.
    if parts[:1] == ('private',):parts=parts[1:]
    if any(part.startswith(('+', '@')) or part == 'private' for part in parts) or any((parent / 'private').exists() for parent in path.parents if parent != Path(path.anchor)):
        return file_fallback(source, 'file-hierarchy', 'Private folders and function or package hierarchies run unchanged.')
    if len(source.document.encode('utf-8')) > MAX_BYTES:
        return file_fallback(source, 'file-size', 'The saved file exceeds the adaptation size limit.')
    scan = lex_source(source.document)
    # Any declaration changes loading scope, including script-local functions.
    if any(d.code == 'unsupported-statement' and source.document[d.start:d.end] in ('function', 'classdef') for d in scan.diagnostics) or any(t.kind == 'identifier' and t.value in ('function', 'classdef') for t in scan.tokens):
        return file_fallback(source, 'file-declaration', 'Function, class and local-function files run unchanged.')
    return adapt_source(source.document, profile=profile)

def map_file_errors(adaptation, context, physical_path, frames, *, native=False):
    """Map matching entry frames only; raw engine messages/frames stay intact."""
    def physical_lines(text):
        lines=re.findall(r'[^\r\n]*(?:\r\n|\r|\n|$)',text)
        if len(lines)>1 and not lines[-1]:lines.pop()
        return lines
    lines = physical_lines(context['document'])
    generated_lines = physical_lines(adaptation.generated_text)
    locations = []
    seen = set()
    for frame in frames or []:
        if not isinstance(frame, dict) or frame.get('file') != str(context['path'] if native else physical_path):
            continue
        line = frame.get('line')
        if type(line) is not int or not 1 <= line <= len(lines):
            continue
        start = len(''.join(lines[:line-1]).encode('utf-16-le')) // 2
        end = start + len(lines[line-1].rstrip('\r\n').encode('utf-16-le')) // 2
        item = {'path': context['path'], 'line': line, 'column': 1,
                'start_utf16': start, 'end_utf16': end,
                'line_only': True, 'adapted': False}
        column = frame.get('column')
        if not native and adaptation.status == 'adapted' and type(column) is int and column > 0 and line <= len(generated_lines):
            local = column - 1
            if local < len(generated_lines[line-1].encode('utf-8')):
                offset = len(''.join(generated_lines[:line-1]).encode('utf-8')) + local
                try:
                    span, rewritten = adaptation.source_map.locate(offset, units='utf8')
                    item.update(start_utf16=span.start_utf16, end_utf16=span.end_utf16,
                                column=span.start_utf16-start+1, line_only=False, adapted=rewritten)
                except ValueError:
                    pass
        key = (item['line'], item['column'], item['line_only'])
        if key not in seen:
            seen.add(key)
            locations.append(item)
    return locations
