"""Editor snapshot contract and job-bound original source locations.

No generated paths/maps are accepted from clients. UTF-16 is the editor
boundary; measured Octave `near line ..., column ...` locations use UTF-8.
"""
import bisect
import hashlib
import re

from backend.i18n import tr
from backend.source_adapter import adapt_source
from backend.source_lexer import lex_source

ORIGINS = {'editor-selection', 'editor-section', 'editor-advance', 'editor-to-end'}


def validate_source_context(value, code, mode, workspace):
    if not isinstance(value, dict) or set(value) != {'origin', 'document', 'span', 'path', 'revision', 'profile', 'cursor'}:
        raise ValueError(tr('Invalid editor source context.'))
    if mode != 'code' or value['origin'] not in ORIGINS or value['profile'] != 'matlab':
        raise ValueError(tr('Source adaptation is limited to editor selections and sections.'))
    document, path = value['document'], value['path']
    if not isinstance(path, str) or not path or len(path) > 4096:
        raise ValueError(tr('Invalid editor source identity.'))
    path = str(workspace.path(path))  # Dirty and untitled .m paths need not exist.
    if not path.endswith('.m') or not isinstance(value['revision'], str) or len(value['revision']) > 128:
        raise ValueError(tr('Invalid editor source identity.'))
    # The disabled adapter validates byte/UTF-16 bounds and derives the text.
    unit = adapt_source(document, value['span']).generated_text
    cursor = value['cursor']
    total = len(document.encode('utf-16-le')) // 2
    if type(cursor) is not int or not 0 <= cursor <= total:
        raise ValueError(tr('Invalid editor source context.'))
    if unit != code:
        raise ValueError(tr('Editor source span does not match the submitted code.'))
    span = list(value['span'])
    if value['origin'] != 'editor-selection':
        # On a successful authoritative scan, derive %% boundaries on the
        # server. If the snapshot is lexically uncertain, retain the submitted
        # span for whole-unit fallback; it cannot authorize any rewrite.
        scan = lex_source(document)
        if scan.ok:
            headers = [0]
            for token in scan.tokens:
                if token.kind == 'comment' and document[token.start:token.end].startswith('%%'):
                    start = max(document.rfind('\n', 0, token.start), document.rfind('\r', 0, token.start)) + 1
                    if not document[start:token.start].strip():
                        headers.append(len(document[:start].encode('utf-16-le')) // 2)
            headers = sorted(set(headers))
            start = max(offset for offset in headers if offset <= cursor)
            following = next((offset for offset in headers if offset > start), total)
            end = total if value['origin'] == 'editor-to-end' else following
            # Legacy section runs exclude the separator newline before the
            # next header. The final section retains its terminal newline.
            raw = document.encode('utf-16-le')[start * 2:end * 2].decode('utf-16-le')
            if following < total and value['origin'] != 'editor-to-end':
                raw = raw.removesuffix('\n')
            derived = [start, start + len(raw.encode('utf-16-le')) // 2]
            if derived != span:
                raise ValueError(tr('Editor section boundaries changed; submit the current snapshot again.'))
    return {**value, 'path': path, 'span': span,
            'snapshot_sha256': hashlib.sha256(document.encode('utf-8')).hexdigest()}


def map_error(adaptation, context, message):
    """Map only engine-reported locations; stdout is never rewritten."""
    locations = []
    lines = re.findall(r'[^\r\n]*(?:\r\n|\r|\n|$)', adaptation.generated_text)
    if lines and not lines[-1]: lines.pop()
    # evalin resets its parser between top-level statements. Its runtime line
    # is relative to the current parse, while its column remains a physical
    # UTF-8 byte column. There is no submitted-source frame in err.stack.
    # For undefined names, recover a line only from a unique identifier token
    # at the engine's byte column, never from text inside a literal/comment.
    undefined = re.match(r"^'([A-Za-z_]\w*)' undefined near line (\d+), column (\d+)", message)
    scan = lex_source(adaptation.generated_text) if undefined and len(lines) > 1 else None
    line_starts = [0]
    for text in lines:
        line_starts.append(line_starts[-1] + len(text))

    def replace(match):
        line, column = int(match[1]), int(match[2])
        if len(lines) > 1:
            # Other runtime messages provide no evidence for the parser's
            # starting line. The syntax-caret path below has its own exact
            # line evidence and bypasses this runtime-only resolution.
            if not undefined or not scan or not scan.ok:
                return match[0]
            candidates = []
            for token in scan.tokens:
                if token.kind != 'identifier' or token.value != undefined[1]:
                    continue
                index = bisect.bisect_right(line_starts, token.start) - 1
                byte_column = len(lines[index][:token.start - line_starts[index]].encode('utf-8')) + 1
                if index + 1 >= line and byte_column == column:
                    candidates.append(index + 1)
            if len(candidates) != 1:
                return match[0]
            line = candidates[0]
        return locate(line, column, match[0])

    def locate(line, column, unchanged):
        if not 1 <= line <= len(lines) or column < 1:
            return unchanged
        # Do not let an invalid column spill into the next generated line.
        if column > len(lines[line - 1].rstrip('\r\n').encode('utf-8')):
            return unchanged
        offset = sum(len(text.encode('utf-8')) for text in lines[:line - 1]) + column - 1
        try:
            span, generated = adaptation.source_map.locate(offset, units='utf8')
        except ValueError:
            return unchanged
        before = context['document'].encode('utf-16-le')[:span.start_utf16 * 2].decode('utf-16-le')
        original_line = len(re.findall(r'\r\n|\r|\n', before)) + 1
        tail = re.split(r'\r\n|\r|\n', before)[-1]
        original_column = len(tail.encode('utf-16-le')) // 2 + 1
        locations.append({'line': original_line, 'column': original_column,
                          'start_utf16': span.start_utf16, 'end_utf16': span.end_utf16,
                          'adapted': generated})
        return f'near line {original_line}, column {original_column}'
    mapped = re.sub(r'near line (\d+), column (\d+)', replace, message)
    # evalin parse errors sometimes omit a line/column but supply a source
    # line and caret. Resolve only a unique exact generated line; never guess
    # which repeated line failed or assign a location to a bare error message.
    syntax = re.search(r'(?m)^>>> (.*)\n( *)\^', message)
    if syntax and not locations:
        matches = [index for index, text in enumerate(lines) if text.rstrip('\r\n') == syntax[1]]
        if len(matches) == 1 and len(syntax[2]) >= 4:
            generated_line = matches[0] + 1
            # The engine includes the four-character >>> prefix in its caret.
            column = len(syntax[2]) - 4 + 1
            locate(generated_line, column, '')
            if locations:
                location = locations[0]
                original = re.split(r'\r\n|\r|\n', context['document'])[location['line'] - 1]
                mapped = mapped[:syntax.start()] + f">>> {original}\n{' ' * (location['column'] + 3)}^" + mapped[syntax.end():]
    return mapped, locations
