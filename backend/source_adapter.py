"""Opt-in, all-or-nothing MATLAB expression literal adaptation.

Public spans use absolute editor UTF-16 offsets (half-open); no file execution
or application integration is performed here. Package verification is explicit,
read-only, and never loads/patches a user's running session. A verified profile
is an ephemeral capability: callers must reverify after path/package changes.
"""
from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import bisect
from pathlib import Path
import subprocess
import tempfile
import time

from backend.source_lexer import Diagnostic, MAX_BYTES, lex_source

ADAPTER_VERSION = 'editor-literals-1a.2'
CONSTRUCTOR_SHA256 = 'c7c26e242f7602cbb2ab6e73b7a601d1d892c2347d3cb9c84eac684513b71960'
MAX_MAP_PIECES = 100_000
TIME_LIMIT = 1.0

# Published stage-1 dispatch table. Values are recorded MATLAB probe IDs whose
# summaries were reproduced with octave-cli and object arguments. This is a
# lexical allowlist, not a claim that every overload/options/shape is supported.
# Additions require measured evidence; existence of a method is insufficient.
STRING_AWARE = {
    'string': ('yd-string-char-empty',),
    'char': ('yd-cast-char-row', 'ym-sortrows-char', 'ym-bin2dec-char-matrix'),
    'cellstr': ('yd-string-cellstr-convert',),
    'double': ('yd-string-double-convert',),
    'isempty': ('yd-string-isempty',),
    'ismissing': ('yd-string-is-missing', 'ys-string-missing'),
    'strlength': ('yd-string-strlength', 'ys-strlength-string'),
    'join': ('yd-string-join', 'ys-join-string', 'ys-string-join-mixed'),
    'split': ('yd-string-split',),
    'reshape': ('yd-string-reshape',),
    'strcmp': ('ys-strcmp-mixed-string',),
    'contains': ('ys-contains-string', 'ys-contains-string-pattern', 'ys-string-mixed-contains'),
    'endsWith': ('ys-endswith-string-cell',),
    'lower': ('ys-lower-string-cell',),
    'numel': ('ys-numel-string',),
    # Shape calibration: use the class|size fields of the literal summaries.
    'size': ('tohum-cift-tirnak', 'yd-string-strings-size'),
    'extract': ('ys-digitspattern-basic', 'ys-letterspattern-basic', 'ys-whitespacepattern', 'ys-pattern-optional'),
    'optionalPattern': ('ys-optionalpattern',),
    'replace': ('ys-pattern-replace', 'ys-replace-string-pattern'),
    'startsWith': ('ys-startswith-string-pattern',),
    'count': ('ys-count-string-pattern',),
    'pad': ('ys-pad-string-left',),
    'extractBefore': ('ys-extractbefore-string',),
    'extractAfter': ('ys-extractafter-string',),
    'splitlines': ('ys-splitlines-string',),
    'reverse': ('ys-reverse-string',),
    'insertBefore': ('ys-insertbefore-string', 'ys-insertbefore-pattern'),
    'insertAfter': ('ys-insertafter-string', 'ys-insertafter-pattern'),
}

# Known text/flag callees, distinct from unresolved handles, fields, indexing
# and user functions. Their direct literals become single-quoted char payloads.
# str2double has no object method: its measured cell-of-strings case retains
# objects inside the cell, whereas a direct string array must fall back.
CHAR_CALLEES = frozenset('''
idivide sort sortrows sum prod cumsum cumprod mean median std var max min range any
unique union intersect setdiff setxor ismember issorted histcounts movmean
movmedian movmax null norm eig svd qr lu chol cond conv interp1 interp2 integral
odeset corrcoef normalize bounds ifft ifft2 conv2 num2str mat2str sprintf
bin2dec hex2dec base2dec str2double str2num datestr datevec addtodate weekday
dateshift between strfind extractBetween compose struct fprintf error warning
disp eval evalin str2func func2str
'''.split())

# Full-suite comparison of char versus fallback is supplemented with unresolved
# versions of recorded probes. Char produced no additional matches, so the
# unknown default stays fail-closed rather than asserting unmeasured semantics.
UNKNOWN_CALL_POLICY = 'fallback'
SPRINTF_EVIDENCE = ('ym-sprintf-float', 'ym-sprintf-matrix', 'ym-sprintf-width',
                    'ym-sprintf-integer-precision')


@dataclass(frozen=True)
class PackageSupport:
    verified: bool = False
    constructor_path: str = ''
    fingerprint: str = ''
    reason: str = 'Constructor/package support has not been verified.'


@dataclass(frozen=True)
class AdapterProfile:
    enabled: bool = False
    dialect: str = 'matlab'
    package: PackageSupport = PackageSupport()


@dataclass(frozen=True)
class SourceSpan:
    start_utf16: int
    end_utf16: int


@dataclass(frozen=True)
class Replacement:
    original: SourceSpan
    generated_start_utf16: int
    generated_end_utf16: int
    original_text: str
    generated_text: str


@dataclass(frozen=True)
class MapPiece:
    # Generated offsets are local to this execution unit; original offsets are
    # absolute in the submitted document. Lines/columns are one-based UTF-16.
    generated_start_utf16: int
    generated_end_utf16: int
    generated_start_utf8: int
    generated_end_utf8: int
    original_start_utf16: int
    original_end_utf16: int
    generated_line: int
    generated_column_utf16: int
    original_line: int
    original_column_utf16: int
    kind: str  # exact / payload / scaffolding


@dataclass(frozen=True)
class SourceMap:
    pieces: tuple[MapPiece, ...]

    def locate(self, offset, *, units='utf16'):
        """Map a generated offset; scaffolding selects the rewritten literal/call.

        Callers must specify UTF-16 or UTF-8. A runtime-column calibration
        fixture measures UTF-8 byte columns for the tested Octave build; this
        API never assumes that contract for every engine/error source.
        """
        if units not in ('utf16', 'utf8'):
            raise ValueError('Source map requires explicit utf16 or utf8 units.')
        start_key = 'generated_start_' + units
        end_key = 'generated_end_' + units
        for piece in self.pieces:
            start, end = getattr(piece, start_key), getattr(piece, end_key)
            if start <= offset < end:
                                return SourceSpan(piece.original_start_utf16, piece.original_end_utf16), piece.kind != 'exact'
        raise ValueError('Generated offset is outside the source map.')


@dataclass(frozen=True)
class Adaptation:
    generated_text: str
    replacements: tuple[Replacement, ...]
    source_map: SourceMap
    diagnostics: tuple[Diagnostic, ...]
    adapter_version: str
    status: str  # unchanged / adapted / fallback
    span: SourceSpan
    package_fingerprint: str = ''

    def metadata(self):
        return asdict(self)


def verify_package(constructor, *, executable, setup='', cwd=None, timeout=15) -> PackageSupport:
    """Verify supported bytes AND Octave's resolved @string handle in isolation.

    setup/cwd are trusted application setup, never editor source. Executable
    must be octave-cli; no shell, package install, patch or persistent session.
    """
    path = Path(constructor)
    try:
        if Path(executable).name != 'octave-cli':
            return PackageSupport(reason='Verification requires octave-cli.')
        if path.stat().st_size > 2_000_000:
            return PackageSupport(reason='Constructor exceeds verification size bound.')
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != CONSTRUCTOR_SHA256:
            return PackageSupport(reason='Absent, unpatched or unsupported datatypes constructor.')
        path = path.resolve(strict=True)
        quote = lambda value: "'" + str(value).replace("'", "''") + "'"
        code = setup + "\ntry\n"
        code += "h = @string; info = functions(h);\n"
        code += f"assert(strcmp((@which)('string'), {quote(path)}));\n"
        code += f"assert(isempty(info.file) || strcmp(info.file, {quote(path)}));\n"
        code += "s = h('a\\n'); assert(isa(s,'string')); assert(isequal(char(s), 'a\\n'));\n"
        code += "e = h(''); assert(isa(e,'string')); assert(isequal(size(e),[1 1])); assert(isequal(size(char(e)),[0 0]));\n"
        code += "fprintf('@@ADAPTER_OK\\n');\ncatch err\nfprintf('@@ADAPTER_FAIL %s\\n',err.message);\nend\n"
        with tempfile.TemporaryDirectory(prefix='source-constructor-') as temp:
            script = Path(temp, 'verify.m')
            script.write_text(code, encoding='utf-8')
            done = subprocess.run([str(executable), '--quiet', '--no-init-file', '--no-site-file', '--no-history', str(script)],
                                  cwd=cwd or temp, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT, text=True, timeout=timeout)
        if done.returncode != 0 or '@@ADAPTER_OK' not in done.stdout.splitlines():
            return PackageSupport(reason='Resolved constructor or literal semantics failed verification.')
        return PackageSupport(True, str(path), 'datatypes-1.5.0:sha256:' + digest, '')
    except (OSError, subprocess.SubprocessError):
        return PackageSupport(reason='Constructor verification unavailable or timed out.')


class _MapBudget(Exception):
    pass


class _PlanBudget(Exception):
    pass


def _positions(text):
    """Linear tables: code point -> UTF-16, UTF-8 and physical line/column."""
    u16 = [0]
    u8 = [0]
    lines = [1]
    columns = [1]
    for i, c in enumerate(text):
        width = 2 if ord(c) > 0xffff else 1
        u16.append(u16[-1] + width)
        u8.append(u8[-1] + len(c.encode('utf-8')))
        newline = c == '\n' or c == '\r' and text[i + 1:i + 2] != '\n'
        lines.append(lines[-1] + int(newline))
        columns.append(1 if newline else columns[-1] + width)
    return u16, u8, lines, columns


def _literal_plan(tokens, deadline):
    """Resolve only bounded lexical positions; never look up a user's names.

    Return literal kinds and result-wrap spans, or a whole-unit diagnostic.
    Parentheses around an argument are expressions, not direct literals. Cells
    keep their element types; a string array cannot be lowered to a char array.
    A known name assigned anywhere in the snapshot is also ambiguous indexing.
    """
    trivia = {'space', 'newline', 'comment', 'block', 'continuation'}
    ts = [t for t in tokens if t.kind not in trivia]
    pairs, reverse_pairs, parents, stack = {}, {}, {}, []
    assigned = set()

    def check_budget():
        if time.monotonic() > deadline:
            raise _PlanBudget

    for i, t in enumerate(ts):
        if i % 1024 == 0 and time.monotonic() > deadline:
            return {}, (), Diagnostic('time-limit', 'Call-position budget exhausted.', t.start, t.end)
        parents[i] = stack[-1] if stack else None
        if t.kind == 'open':
            stack.append(i)
        elif t.kind == 'close':
            opening = stack.pop()
            parents[i] = stack[-1] if stack else None
            pairs[opening] = i
            reverse_pairs[i] = opening
            if ts[opening].value == '(' and opening and ts[opening - 1].value == '@':
                assigned.update(x.value for x in ts[opening + 1:i] if x.kind == 'identifier')
        elif t.value == '=' and i and ts[i - 1].kind == 'identifier':
            assigned.add(ts[i - 1].value)
        elif t.value == '=' and i and ts[i - 1].value == ']':
            # Multiple output binding. No function/command declarations are
            # admitted by the lexer, so these are the other simple bindings.
            opening = reverse_pairs[i - 1]
            assigned.update(x.value for x in ts[opening:i] if x.kind == 'identifier')
        elif t.value == '=' and i and ts[i - 1].kind == 'close':
            a = i - 1
            while a in reverse_pairs:
                a = reverse_pairs[a] - 1
            if a >= 0 and ts[a].kind == 'identifier':
                assigned.add(ts[a].value)
    kinds = {t.start: 'string' for t in ts if t.kind == 'double'}
    wraps = []
    counts = [0]
    for t in ts:
        counts.append(counts[-1] + (t.kind == 'double'))

    def has_literal(a, b):
        return counts[b] != counts[a]

    def failure(code, message, a, b):
        return {}, (), Diagnostic(code, message, ts[a].start, ts[b - 1].end)

    # Innermost calls first. Parent calls may read a char/numeric result from a
    # measured converter, but must not consume a newly string-valued result.
    string_results = {'string', 'join', 'split', 'lower', 'extract', 'replace',
                      'pad', 'extractBefore', 'extractAfter', 'splitlines',
                      'reverse', 'insertBefore', 'insertAfter', 'sprintf'}
    call_results = {}
    calls = []
    for opening in sorted(pairs, reverse=True):
        check_budget()
        closing = pairs[opening]
        if ts[opening].value != '(':
            continue
        previous = ts[opening - 1] if opening else None
        callee, call_start = None, opening
        if previous and previous.kind == 'identifier':
            if previous.value in {'if', 'elseif', 'while', 'switch', 'case', 'catch'}:
                continue
            callee, call_start = previous.value, opening - 1
            if opening > 1 and ts[opening - 2].value in {'.', '@'}:
                callee = None
        elif previous and previous.kind == 'close':
            # Handle/computed expression call or chained indexing.
            callee = None
        elif previous and previous.value == '.':
            callee = None
        else:
            # Grouping and anonymous-function parameter lists, not calls.
            continue
        known = callee in STRING_AWARE or callee in CHAR_CALLEES
        unresolved = not known or callee in assigned
        if unresolved:
            callee = None
        aware = callee in STRING_AWARE
        arguments = []
        a = opening + 1
        i = a
        while i < closing:
            if ts[i].value == ',' and parents[i] == opening:
                arguments.append((a, i))
                a = i + 1
            i = pairs[i] + 1 if i in pairs else i + 1
        arguments.append((a, closing))
        calls.append((opening, closing, callee, unresolved, tuple(arguments)))
        format_literal = (callee == 'sprintf' and arguments
                          and arguments[0][1] == arguments[0][0] + 1
                          and ts[arguments[0][0]].kind == 'double')
        call_results[opening] = (bool(format_literal) if callee == 'sprintf'
                                 else callee in string_results and has_literal(opening + 1, closing))
        for a, b in arguments:
            if not has_literal(a, b):
                continue
            if b == a + 1 and ts[a].kind == 'double':
                if unresolved and UNKNOWN_CALL_POLICY != 'char':
                    return failure('unresolved-string-call', 'String argument in an unresolved call, handle, field or variable indexing expression.', call_start, closing + 1)
                kinds[ts[a].start] = 'string' if aware else 'char'
                continue
            # Direct cell arguments preserve their string elements even for a
            # char-only callee (str2double's recorded cell case depends on it).
            if ts[a].value == '{' and pairs.get(a) == b - 1:
                if unresolved and UNKNOWN_CALL_POLICY != 'char':
                    return failure('unresolved-string-call', 'Cell of string literals in an unresolved call/indexing expression.', call_start, closing + 1)
                continue
            if not aware:
                j = a
                while j < b:
                    if ts[j].kind == 'double' or ts[j].value == '[' and has_literal(j, pairs[j] + 1):
                        if unresolved and UNKNOWN_CALL_POLICY != 'char':
                            return failure('unresolved-string-call', 'String expression in an unresolved call, handle, field or variable indexing expression.', call_start, closing + 1)
                        return failure('string-array-argument' if ts[j].value == '[' else 'string-expression-argument',
                                       f'{callee or "Unresolved callee"}: a string array/expression has no safe char-literal equivalent.', a, b)
                    if j + 1 in call_results:
                        if call_results[j + 1]:
                            return failure('string-result-argument', f'{callee or "Unresolved callee"}: consuming a string-valued call result is not verified.', a, b)
                        j = pairs[j + 1] + 1
                    else:
                        j += 1
        if format_literal:
            wraps.append((ts[call_start].start, ts[closing].end))

    # Conservative lexical forwarding guard. It never rewrites variables; it
    # only rejects units that hand a newly object-valued binding to an API whose
    # object dispatch is unmeasured. This also covers stored format literals.
    boundaries = []
    depth, continued = 0, False
    for t in tokens:
        if t.kind == 'open':
            depth += 1
        elif t.kind == 'close':
            depth -= 1
        elif t.kind == 'continuation':
            continued = True
        elif t.kind == 'newline':
            if not depth and not continued:
                boundaries.append(t.start)
            continued = False
        elif t.kind == 'separator' and not depth:
            boundaries.append(t.start)
    starts = [t.start for t in ts]
    ends = [bisect.bisect_left(starts, p) for p in boundaries] + [len(ts)]
    bindings, writes, loops = [], [], []
    for i, t in enumerate(ts):
        if i % 1024 == 0:
            check_budget()
        if t.value != '=':
            continue
        b = ends[bisect.bisect_right(ends, i)]
        if i and ts[i - 1].kind == 'identifier':
            bindings.append((ts[i - 1].value, i + 1, b))
        a = i - 1
        boundary = ends[bisect.bisect_left(ends, i) - 1] if bisect.bisect_left(ends, i) else 0
        while a >= boundary and ts[a].kind != 'separator' and parents[a] == parents[i]:
            if a % 1024 == 0:
                check_budget()
            if a in reverse_pairs:
                a = reverse_pairs[a] - 1
                continue
            if ts[a].value == '.':
                writes.append((a, i + 1, b))
                break
            if ts[a].value in {'for', 'parfor'}:
                loops.append((a, i + 1, b))
                break
            a -= 1

    def object_in(a, b, tainted):
        j = a
        while j < b:
            if j % 1024 == 0:
                check_budget()
            if ts[j].kind == 'double' and kinds[ts[j].start] == 'string':
                return True
            if j + 1 in call_results:
                if call_results[j + 1]:
                    return True
                # Known char/numeric converters suppress argument provenance.
                if ts[j].value in {'char', 'double', 'cellstr', 'numel', 'isempty',
                                    'ismissing', 'strlength', 'size', 'strcmp', 'contains',
                                    'endsWith', 'startsWith', 'count', 'optionalPattern'}:
                    j = pairs[j + 1] + 1
                    continue
            if ts[j].kind == 'identifier' and ts[j].value in tainted:
                return True
            j += 1
        return False

    # A graph reachability pass is bounded by the name/token budgets. Scan to a
    # fixed point with an explicit deadline; bindings are conservatively merged
    # across branches/reassignments rather than making control-flow promises.
    tainted = set()
    while True:
        before = len(tainted)
        for name, a, b in bindings:
            if time.monotonic() > deadline:
                return failure('time-limit', 'String forwarding budget exhausted.', a - 1, max(a, b))
            if name not in tainted and object_in(a, b, tainted):
                tainted.add(name)
        if len(tainted) == before:
            break
    for a, rhs, b in writes:
        if object_in(rhs, b, tainted):
            return failure('string-property-write', 'A string-valued property/field write needs object-specific assignment semantics.', a, b)
    for a, rhs, b in loops:
        if object_in(rhs, b, tainted):
            return failure('string-iteration', 'String-valued iteration needs an unsupported object iteration protocol.', a, b)
    for opening, closing, callee, unresolved, arguments in calls:
        if callee in STRING_AWARE:
            continue
        for a, b in arguments:
            # Literal cells retain their element types by rule (b); don't
            # mistake a cell container for a direct string expression.
            if a < b and ts[a].value == '{' and pairs.get(a) == b - 1:
                continue
            if object_in(a, b, tainted):
                return failure('string-forwarding', f'{callee or "Unresolved callee"}: a string-valued operand/binding is not a direct flag literal; object dispatch is unverified.', opening, closing + 1)
    return kinds, tuple(wraps), None


def adapt_source(document: str, span=None, profile: AdapterProfile | None = None) -> Adaptation:
    """Adapt a validated immutable editor snapshot; default profile is disabled.

    None span means the whole snapshot. Tuple spans and SourceSpan use UTF-16,
    never Python indices. Oversized input and invalid/surrogate-splitting bounds
    raise ValueError before adaptation; callers must reject that submission.
    Lexical uncertainty anywhere in the document is conservatively a fallback.
    """
    profile = profile or AdapterProfile()
    if not isinstance(profile, AdapterProfile) or not isinstance(document, str):
        raise ValueError('Expected text and an AdapterProfile.')
    if len(document) > MAX_BYTES or len(document.encode('utf-8', errors='surrogatepass')) > MAX_BYTES:
        raise ValueError('Document exceeds the source byte limit.')
    if any(0xd800 <= ord(c) <= 0xdfff for c in document):
        raise ValueError('Document contains an unpaired surrogate.')
    deadline = time.monotonic() + TIME_LIMIT
    original_u16, _, original_lines, original_columns = _positions(document)
    if span is not None and not isinstance(span, SourceSpan) and not (isinstance(span, (tuple, list)) and len(span) == 2):
        raise ValueError('Expected a half-open UTF-16 span.')
    selected = SourceSpan(0, original_u16[-1]) if span is None else span if isinstance(span, SourceSpan) else SourceSpan(*span)
    if (type(selected.start_utf16) is not int or type(selected.end_utf16) is not int
            or not 0 <= selected.start_utf16 <= selected.end_utf16 <= original_u16[-1]):
        raise ValueError('Invalid UTF-16 selection bounds.')
    start = bisect.bisect_left(original_u16, selected.start_utf16)
    end = bisect.bisect_left(original_u16, selected.end_utf16)
    if original_u16[start] != selected.start_utf16 or original_u16[end] != selected.end_utf16:
        raise ValueError('Selection splits a UTF-16 surrogate pair.')
    unit = document[start:end]
    replacements = []
    pieces = []
    output = []
    generated_u16 = generated_u8 = 0
    generated_line = generated_column = 1

    def append(text, source_start, source_end, kind):
        nonlocal generated_u16, generated_u8, generated_line, generated_column
        # Exact text is split per character, giving exact UTF-8 and supplementary
        # Unicode maps, and naturally a per-line piecewise map. Scaffolding is
        # one piece; it never contains a physical newline.
        entries = ((c, source_start + i, source_start + i + 1) for i, c in enumerate(text)) if kind == 'exact' else ((text, source_start, source_end),)
        for value, a, b in entries:
            if len(pieces) >= MAX_MAP_PIECES or len(pieces) % 1024 == 0 and time.monotonic() > deadline:
                raise _MapBudget
            width16 = sum(2 if ord(c) > 0xffff else 1 for c in value)
            width8 = len(value.encode('utf-8'))
            pieces.append(MapPiece(generated_u16, generated_u16 + width16, generated_u8, generated_u8 + width8,
                                   original_u16[a], original_u16[b], generated_line, generated_column,
                                   original_lines[a], original_columns[a], kind))
            generated_u16 += width16
            generated_u8 += width8
            if value == '\n' or value == '\r' and document[b:b + 1] != '\n':
                generated_line += 1
                generated_column = 1
            else:
                generated_column += width16
        output.append(text)

    def unchanged(status, diagnostics=()):
        # Maps are bounded even for fallback. Large identity projections can be
        # omitted with a visible map-budget diagnostic; source text stays exact.
        pieces.clear()
        output.clear()
        nonlocal generated_u16, generated_u8, generated_line, generated_column
        generated_u16 = generated_u8 = 0
        generated_line = generated_column = 1
        if len(unit) <= MAX_MAP_PIECES and time.monotonic() <= deadline:
            try:
                append(unit, start, end, 'exact')
            except _MapBudget:
                pieces.clear()
                diagnostics = tuple(diagnostics) + (Diagnostic('map-limit', 'Identity source map omitted at resource limit.', selected.start_utf16, selected.end_utf16),)
        else:
            diagnostics = tuple(diagnostics) + (Diagnostic('map-limit', 'Identity source map omitted at resource limit.', selected.start_utf16, selected.end_utf16),)
        return Adaptation(unit, (), SourceMap(tuple(pieces)), tuple(diagnostics), ADAPTER_VERSION, status, selected, profile.package.fingerprint)

    def located(diagnostic):
        return Diagnostic(diagnostic.code, diagnostic.message, original_u16[min(diagnostic.start, len(document))], original_u16[min(diagnostic.end, len(document))])

    if not profile.enabled or profile.dialect == 'native-octave':
        return unchanged('unchanged')
    lexed = lex_source(document, profile.dialect, deadline=deadline)
    if not lexed.ok:
        return unchanged('fallback', tuple(located(d) for d in lexed.diagnostics))
    for token in lexed.tokens:
        if any(token.start < boundary < token.end for boundary in (start, end)) and token.kind != 'space':
            return unchanged('fallback', (located(Diagnostic('selection-token', 'Selection boundary crosses a source token.', token.start, token.end)),))
    # A selection may cut delimiters/logical context without cutting any token.
    # Recheck the unit independently, but only full-document tokens authorize
    # literal replacements (never reinterpret a selection starting inside text).
    unit_scan = lex_source(unit, profile.dialect, deadline=deadline)
    if not unit_scan.ok:
        return unchanged('fallback', tuple(located(Diagnostic(d.code, d.message, d.start + start, d.end + start)) for d in unit_scan.diagnostics))
    literals = [t for t in lexed.tokens if t.kind == 'double' and start <= t.start and t.end <= end]
    if not literals:
        return unchanged('unchanged')
    if not profile.package.verified or not profile.package.fingerprint or not profile.package.constructor_path:
        return unchanged('fallback', (located(Diagnostic('package-unverified', profile.package.reason or 'Constructor support not verified.', literals[0].start, literals[0].end)),))
    try:
        kinds, wraps, diagnostic = _literal_plan(lexed.tokens, deadline)
    except _PlanBudget:
        return unchanged('fallback', (located(Diagnostic('time-limit', 'Call-position/forwarding budget exhausted.', start, end)),))
    if diagnostic:
        return unchanged('fallback', (located(diagnostic),))
    for a, b in wraps:
        if a < end and b > start and not start <= a < b <= end:
            return unchanged('fallback', (located(Diagnostic('selection-call', 'Selection cuts a call whose result requires a string wrapper.', a, b)),))
    # A selected literal must not inherit a call position outside its unit.
    try:
        unit_kinds, _, diagnostic = _literal_plan(unit_scan.tokens, deadline)
    except _PlanBudget:
        return unchanged('fallback', (located(Diagnostic('time-limit', 'Call-position/forwarding budget exhausted.', start, end)),))
    if diagnostic:
        return unchanged('fallback', (located(Diagnostic(diagnostic.code, diagnostic.message, diagnostic.start + start, diagnostic.end + start)),))
    if any(kinds[t.start] != unit_kinds.get(t.start - start) for t in literals):
        return unchanged('fallback', (located(Diagnostic('selection-context', 'Selection excludes the call that determines a literal argument type.', literals[0].start, literals[-1].end)),))
    estimated_pieces = len(unit) + sum(2 + t.value.count("'") for t in literals) + 2 * len(wraps)
    if estimated_pieces > MAX_MAP_PIECES:
        return unchanged('fallback', (located(Diagnostic('map-limit', 'Adapted source map exceeds resource limit.', start, end)),))
    try:
        cursor = start
        events = [(t.start, 2, t.end, t, '') for t in literals]
        for a, b in wraps:
            if start <= a < b <= end:
                events.extend(((a, 1, a, None, '(@string)(', a, b), (b, 0, b, None, ')', a, b)))
        for event in sorted(events, key=lambda e: (e[0], e[1])):
            position, _, following, token, insertion = event[:5]
            if time.monotonic() > deadline:
                return unchanged('fallback', (located(Diagnostic('time-limit', 'Adapter time budget exhausted.', position, following)),))
            append(document[cursor:position], cursor, position, 'exact')
            generated_start = generated_u16
            if token is None:
                append(insertion, event[5], event[6], 'scaffolding')
                replacements.append(Replacement(SourceSpan(original_u16[position], original_u16[position]), generated_start, generated_u16, '', insertion))
                cursor = following
                continue
            prefix, suffix = ("(@string)('", "')") if kinds[token.start] == 'string' else ("'", "'")
            append(prefix, token.start, token.end, 'scaffolding')
            for value, offset in zip(token.value, token.payload_offsets):
                source_end = offset + (2 if document[offset] == '"' else 1)
                append(value.replace("'", "''"), offset, source_end, 'payload')
            append(suffix, token.start, token.end, 'scaffolding')
            generated = prefix + token.value.replace("'", "''") + suffix
            replacements.append(Replacement(SourceSpan(original_u16[token.start], original_u16[token.end]), generated_start, generated_u16, document[token.start:token.end], generated))
            cursor = token.end
        append(document[cursor:end], cursor, end, 'exact')
    except _MapBudget:
        return unchanged('fallback', (located(Diagnostic('map-limit', 'Adapter map/time budget exhausted.', start, end)),))
    diagnostics = []
    limitation_names = {
        'sprintf', 'fprintf', 'str2double', 'char', 'extractBetween', 'eval', 'evalin', 'str2func', 'func2str',
        'idivide', 'sort', 'sum', 'prod', 'cumsum', 'cumprod', 'mean', 'median', 'std', 'var',
        'max', 'min', 'range', 'any', 'unique', 'union', 'intersect', 'setdiff', 'setxor',
        'ismember', 'issorted', 'movmean', 'movmedian', 'movmax', 'null', 'norm', 'eig', 'qr',
        'lu', 'chol', 'cond', 'conv', 'conv2', 'interp1', 'interp2', 'integral', 'corrcoef',
        'normalize', 'bounds', 'ifft', 'num2str', 'mat2str', 'bin2dec', 'hex2dec', 'str2num',
        'datestr', 'datevec', 'addtodate', 'datetime', 'dateshift', 'strfind', 'for',
    }
    for token in lexed.tokens:
        if start <= token.start < end and token.kind == 'identifier' and token.value in limitation_names:
            diagnostics.append(located(Diagnostic('semantic-limit', f'{token.value}: only measured literal positions are adapted; other overloads, dynamic source or reflection can differ.', token.start, token.end)))
    if any(ord(c) > 0xffff for t in literals for c in t.value):
        diagnostics.append(located(Diagnostic('unicode-limit', 'Supplementary Unicode operations may differ from MATLAB UTF-16 semantics.', literals[0].start, literals[-1].end)))
    if time.monotonic() > deadline:
        return unchanged('fallback', (located(Diagnostic('time-limit', 'Adapter time budget exhausted.', start, end)),))
    return Adaptation(''.join(output), tuple(replacements), SourceMap(tuple(pieces)), tuple(diagnostics), ADAPTER_VERSION, 'adapted', selected, profile.package.fingerprint)
