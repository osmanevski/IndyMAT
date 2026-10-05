"""Bounded stage-1 lexical grammar, not a general MATLAB parser.

Offsets are Python code points into the untouched document. The adapter converts
these to editor UTF-16/file UTF-8 explicitly. No source is executed here.
Accepted: expression scripts, ordinary control statements, ASCII identifiers,
number literals, handles/anonymous functions, indexing, matrices and cells.
Unsupported declarations, ambiguous command/operator spacing, incomplete tokens,
unbalanced delimiters and cross-line literals fail closed for the whole unit.
"""
from __future__ import annotations

from dataclasses import dataclass
import re
import time

MAX_BYTES = 500_000
MAX_TOKENS = 100_000
MAX_DEPTH = 128
TIME_LIMIT = .5
PROFILES = ('matlab', 'native-octave')
IDENTIFIER = re.compile(r'[A-Za-z_][A-Za-z_0-9]*')
NUMBER = re.compile(r"(?:\d+(?:\.(?![.*'/\\^])\d*)?|\.\d+)(?:[eE][+-]?\d+)?[ij]?")
OPERATORS = tuple(sorted(('...', ".'", '.*', './', '.\\', '.^', '==', '~=', '<=', '>=', '&&', '||', '+', '-', '*', '/', '\\', '^', '=', '<', '>', '~', '&', '|', ':', '.', '@'), key=len, reverse=True))
CONTROL_OPERAND = {'if', 'elseif', 'while', 'switch', 'case', 'for', 'parfor', 'catch'}
CONTROL_BARE = {'else', 'otherwise', 'try', 'end', 'break', 'continue', 'return'}
UNSUPPORTED = {'function', 'classdef', 'arguments', 'properties', 'methods', 'events', 'enumeration', 'import', 'global', 'persistent', 'spmd', 'unwind_protect', 'end_try_catch', 'endif', 'endfor', 'endwhile', 'endfunction'}


@dataclass(frozen=True)
class Token:
    kind: str
    start: int
    end: int
    value: str = ''
    # Decoded literal payload -> original code-point position (first of a doubled pair).
    payload_offsets: tuple[int, ...] = ()


@dataclass(frozen=True)
class Diagnostic:
    code: str
    message: str
    start: int
    end: int


@dataclass(frozen=True)
class Lexed:
    tokens: tuple[Token, ...]
    diagnostics: tuple[Diagnostic, ...]
    profile: str

    @property
    def ok(self):
        return not self.diagnostics

    def masked(self, source):
        """Source-aligned projection; never authorizes a rewrite after failure."""
        chars = list(source)
        for token in self.tokens:
            if token.kind in {'double', 'char', 'comment', 'block', 'continuation', 'command'}:
                for index in range(token.start, token.end):
                    if chars[index] not in '\r\n':
                        chars[index] = ' '
        return ''.join(chars)


def lex_source(source: str, profile: str = 'matlab', *, deadline=None) -> Lexed:
    tokens = []
    diagnostics = []

    def fail(code, message, start, end=None):
        diagnostics.append(Diagnostic(code, message, start, start + 1 if end is None else end))

    if profile not in PROFILES:
        fail('profile', 'Unknown source profile.', 0, 0)
        return Lexed((), tuple(diagnostics), profile)
    if not isinstance(source, str):
        fail('input', 'Source must be text.', 0, 0)
        return Lexed((), tuple(diagnostics), profile)
    if len(source) > MAX_BYTES or len(source.encode('utf-8', errors='surrogatepass')) > MAX_BYTES:
        fail('size-limit', 'Document exceeds the source byte limit.', 0, 0)
        return Lexed((), tuple(diagnostics), profile)
    deadline = min(deadline, time.monotonic() + TIME_LIMIT) if deadline is not None else time.monotonic() + TIME_LIMIT
    i = 0
    stack = []  # delimiter, opening offset, anonymous parameter list
    blocks = []  # opener character and offset
    expected = True
    statement_start = True
    space = False
    continued = False
    pending_continuation = False
    anonymous = False
    previous = None

    def emit(kind, start, end, value='', offsets=()):
        nonlocal previous
        token = Token(kind, start, end, value, tuple(offsets))
        tokens.append(token)
        if kind not in {'space', 'newline', 'comment', 'block', 'continuation'}:
            previous = token

    while i < len(source):
        if len(tokens) >= MAX_TOKENS or time.monotonic() > deadline:
            fail('resource-limit', 'Lexer token/time budget exhausted.', i)
            break
        c = source[i]
        # Whole-line block markers only, including nested blocks of either style.
        line_start = i == 0 or source[i - 1] in '\r\n'
        if line_start and c not in '\r\n':
            end = i
            while end < len(source) and source[end] not in '\r\n':
                end += 1
            stripped = source[i:end].strip(' \t')
            if stripped in ('%{', '#{', '%}', '#}'):
                if stripped[1] == '{':
                    blocks.append((stripped[0], i))
                    if len(blocks) > MAX_DEPTH:
                        fail('depth-limit', 'Block nesting exceeds limit.', i, end)
                        break
                elif not blocks or blocks[-1][0] != stripped[0]:
                    fail('block-delimiter', 'Unmatched or mixed block comment delimiter.', i, end)
                    break
                else:
                    blocks.pop()
                emit('block', i, end)
                i = end
                continue
            if blocks:
                emit('block', i, end)
                i = end
                continue
        if c in '\r\n':
            if pending_continuation and not continued:
                fail('uncertain-continuation', 'Continuation followed by an empty/comment-only physical line.', i)
                break
            end = i + (2 if source[i:i + 2] == '\r\n' else 1)
            emit('newline', i, end)
            i = end
            if not continued:
                if not stack:
                    expected = True
                    statement_start = True
                    previous = None
                elif stack[-1][0] in '[{':
                    expected = True
            continued = False
            space = True
            continue
        if c in ' \t':
            end = i + 1
            while end < len(source) and source[end] in ' \t':
                end += 1
            emit('space', i, end)
            i = end
            space = True
            continue
        if c in '%#' or source.startswith('...', i):
            end = i
            while end < len(source) and source[end] not in '\r\n':
                end += 1
            kind = 'continuation' if c == '.' else 'comment'
            emit(kind, i, end)
            continued = kind == 'continuation'
            pending_continuation = pending_continuation or continued
            i = end
            continue
        pending_continuation = False
        if c == '"' or c == "'" and (expected or space and stack and stack[-1][0] in '[{'):
            if not expected and not (space and stack and stack[-1][0] in '[{'):
                fail('adjacent-literal', 'Literal adjacent to an operand outside a matrix/cell.', i)
                break
            start = i
            quote = c
            payload = []
            offsets = []
            i += 1
            while i < len(source) and source[i] not in '\r\n':
                if i % 4096 == 0 and time.monotonic() > deadline:
                    break
                if source[i] == quote:
                    if i + 1 < len(source) and source[i + 1] == quote:
                        payload.append(quote)
                        offsets.append(i)
                        i += 2
                        continue
                    i += 1
                    break
                if quote == '"' and profile == 'native-octave' and source[i] == '\\':
                    # Native strings are masked/tokenized only; escape interpretation
                    # belongs to Octave. Never adapt this profile.
                    payload.append(source[i])
                    offsets.append(i)
                    i += 1
                    if i < len(source) and source[i] not in '\r\n':
                        payload.append(source[i])
                        offsets.append(i)
                        i += 1
                    continue
                payload.append(source[i])
                offsets.append(i)
                i += 1
            else:
                fail('unterminated-literal', 'Unterminated or cross-line literal.', start, i)
                break
            if i == 0 or source[i - 1] != quote or time.monotonic() > deadline:
                fail('unterminated-literal', 'Unterminated literal or literal time limit.', start, i)
                break
            emit('double' if quote == '"' else 'char', start, i, ''.join(payload), offsets)
            expected = False
            statement_start = False
        elif c == "'" or source.startswith(".'", i):
            if expected:
                fail('transpose', 'Transpose without a completed operand.', i)
                break
            end = i + (2 if c == '.' else 1)
            emit('transpose', i, end)
            i = end
        elif c in '([{':
            stack.append((c, i, anonymous and c == '('))
            anonymous = False
            if len(stack) > MAX_DEPTH:
                fail('depth-limit', 'Delimiter nesting exceeds limit.', i)
                break
            emit('open', i, i + 1, c)
            i += 1
            expected = True
            statement_start = False
        elif c in ')]}':
            if not stack or stack[-1][0] != {')': '(', ']': '[', '}': '{'}[c]:
                fail('delimiter', 'Unmatched expression delimiter.', i)
                break
            _, _, params = stack.pop()
            emit('close', i, i + 1, c)
            i += 1
            expected = params
        elif c in ',;':
            emit('separator', i, i + 1, c)
            i += 1
            expected = True
            if not stack:
                statement_start = True
                previous = None
        elif IDENTIFIER.match(source, i):
            if not expected and not statement_start and not (space and stack and stack[-1][0] in '[{'):
                fail('adjacent-operand', 'Adjacent identifier without a resolved operator.', i)
                break
            match = IDENTIFIER.match(source, i)
            word = match.group()
            end = match.end()
            if statement_start and word in UNSUPPORTED:
                fail('unsupported-statement', f'Unsupported stage-1 statement: {word}.', i, end)
                break
            if statement_start and word not in CONTROL_OPERAND | CONTROL_BARE:
                tail = end
                while tail < len(source) and source[tail] in ' \t':
                    tail += 1
                next_c = source[tail:tail + 1]
                if tail > end and next_c and next_c not in '=(.,;\r\n%#':
                    # A statement beginning name + spaced operand is command
                    # syntax. Operator spacing is intentionally not guessed.
                    if next_c in '+-*/\\^~<>&|:':
                        fail('ambiguous-command', 'Ambiguous command/operator spacing.', i, tail + 1)
                        break
                    command_end = tail
                    while command_end < len(source) and source[command_end] not in '\r\n':
                        command_end += 1
                    command = source[tail:command_end]
                    if '"' in command or "'" in command:
                        fail('quoted-command', 'Quoted command-form arguments execute unchanged.', i, command_end)
                        break
                    # Simple unquoted command only. Separators/continuations
                    # need a fuller command grammar, so reject conservatively.
                    if any(x in command for x in (';', ',', '...', '%', '#')):
                        fail('command-syntax', 'Unsupported compound command-form statement.', i, command_end)
                        break
                    emit('command', i, command_end, word)
                    i = command_end
                    expected = True
                    space = False
                    statement_start = False
                    continue
            emit('identifier', i, end, word)
            i = end
            expected = statement_start and word in CONTROL_OPERAND
            statement_start = word in CONTROL_BARE
        elif NUMBER.match(source, i):
            if not expected and not (space and stack and stack[-1][0] in '[{'):
                fail('adjacent-operand', 'Adjacent number without a resolved operator.', i)
                break
            match = NUMBER.match(source, i)
            emit('number', i, match.end(), match.group())
            i = match.end()
            expected = False
            statement_start = False
        else:
            operator = next((op for op in OPERATORS if source.startswith(op, i)), None)
            if operator is None:
                fail('unsupported-token', 'Unsupported source token.', i)
                break
            if operator == '.' and (expected or previous is None):
                fail('field-access', 'Unresolved field access.', i)
                break
            emit('operator', i, i + len(operator), operator)
            i += len(operator)
            anonymous = operator == '@'
            expected = True
            statement_start = False
        space = False
    if not diagnostics and blocks:
        fail('unterminated-block', 'Unterminated block comment.', blocks[-1][1], len(source))
    if not diagnostics and stack:
        fail('unterminated-delimiter', 'Unterminated expression delimiter.', stack[-1][1], len(source))
    if not diagnostics and previous is not None and previous.kind == 'operator':
        fail('incomplete-expression', 'Expression ends with an operator.', previous.start, previous.end)
    if not diagnostics and pending_continuation:
        fail('unterminated-continuation', 'Continuation has no following physical line.', max(0, len(source) - 3), len(source))
    return Lexed(tuple(tokens), tuple(diagnostics), profile)
