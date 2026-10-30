---
title: Kernel protocol (Rust engine)
created: 2026-10-29
modified: 2026-10-30
type: contract
status: frozen
---

# Kernel protocol — `indy-kernel` over stdio

The Rust engine runs user code in a child process, `indy-kernel`, that speaks
JSON lines over its stdin/stdout. The Python side is `backend/kernel_rust.py`
(`RustKernel`), which gives `app.py` the same interface as the Octave
`backend/kernel.py`. The engine behind the binary is the `Engine` trait of the
`indy-kernel` crate; `StubEngine` is a tiny, honest placeholder until the phase 2
interpreter implements the trait.

This file is the frozen contract. Changing a message, field or semantic needs
the orchestrator; adding an optional field or a new `op` is allowed when old
clients keep working (unknown fields are ignored by both sides).

## Transport

- The client starts the child with its working directory set to the user's
  workspace folder, stdin and stdout as pipes and stderr as a separate pipe.
- **stdin:** one request per line: a JSON object in UTF-8 terminated by `\n`.
  A line is at most 4 MiB. Blank lines are ignored.
- **stdout:** one message per line: a JSON object in UTF-8 terminated by `\n`,
  flushed after each line. Nothing else is ever written to stdout; user output
  travels inside `output` events.
- **stderr:** free-form diagnostics for logs, never part of the protocol.
- EOF on stdin ends the child (exit status 0), also during a running `eval`.

## Startup

Before reading any request the child writes one message:

```json
{"type":"ready","protocol":1,"engine":{"name":"stub","version":"0.0.1","stub":true,"description":"IndyMAT Rust stub engine (not the real interpreter)"},"cwd":"/Users/me/work"}
```

`engine.stub` is `true` for any engine that is not the real interpreter. The
client must show `engine.description` (for example as the version text) so a
stub is never mistaken for the real engine.

## Requests and replies

A request is `{"id": <integer ≥ 1>, "op": "<name>", ...parameters}`. The
client chooses ids (unique within one child process). The child echoes the id.

**Every request gets exactly one `response`:**

```json
{"type":"response","id":7,"ok":true,"result":{...}}
{"type":"response","id":7,"ok":false,"error":{"code":"unsupported","feature":"debugger","message":"The debugger is not supported in the Rust engine yet."}}
```

Error codes:

| code | meaning |
|---|---|
| `bad_request` | the line is not a JSON object, `id` is missing/invalid, a parameter has the wrong type |
| `unknown_op` | `op` is not part of this protocol |
| `unsupported` | `op` is part of the protocol but this engine does not implement it; `feature` names the UI feature; `message` is user-facing |
| `busy` | an `eval` is running; only `input_reply`, `info` and `close` are accepted meanwhile (during a debugger pause also the requests listed under Debugger) |
| `not_found` | a named variable, file or folder does not exist |
| `invalid` | the request is well-formed but not valid now (e.g. `input_reply` with no input pending, a bad variable name) |
| `engine` | the engine failed to carry out an accepted operation |

`unsupported` is never a silent no-op: the client shows `message`.

## Events (only for `eval`)

An `eval` request is acknowledged by its `response` first (`ok:true`,
`result:{}`) or refused by it (`ok:false`, then nothing else follows). After an
accepted `eval` the child writes zero or more events carrying the same `id`,
and **exactly one terminal event**, `done` or `error`:

```json
{"type":"event","id":9,"event":"output","stream":"stdout","text":"x = 3\n","stub":true}
{"type":"event","id":9,"event":"clc"}
{"type":"event","id":9,"event":"input_request","prompt":"Value? "}
{"type":"event","id":9,"event":"figure","number":1,"name":"Figure 1","figure":{...Figure-Data-v3...},"png_base64":"...","reason":""}
{"type":"event","id":9,"event":"done","cwd":"/Users/me/work"}
{"type":"event","id":9,"event":"error","message":"'y' undefined","interrupted":false,"cwd":"/Users/me/work","location":{"file":"/Users/me/work/a.m","line":3}}
```

- `output`: `stream` is `stdout` or `stderr`; `text` is appended verbatim.
  `stub:true` marks output from a stub engine.
- `clc`: the user cleared the Command Window; the client discards the output of
  this eval received so far.
- `input_request`: user code waits for one line of input (`input`; `keyboard`
  is a `paused` event, see Debugger). The client answers with `input_reply`. An interrupt cancels it.
- `figure`: after each eval the engine reports the **complete list of open
  figures** (at most 12), one event each, in figure-number order. An eval that
  emits no `figure` event leaves no figure open. `figure` is a Figure-Data-v3
  object (`docs/Figure-Data-v3.md`) or `null`; `png_base64` is optional;
  `reason` explains a non-interactive figure.
- `done`: the code finished. `cwd` is the engine's folder afterwards.
- `error`: the code raised an error or was interrupted (`interrupted:true`).
  `location` is optional. Variables assigned before the error stay.

Only workspace data is not pushed: after the terminal event the client asks
for `workspace`.

## Operations

| op | parameters | result |
|---|---|---|
| `info` | — | `{protocol, engine:{name,version,stub,description}, ops:[supported op names], unsupported?:[UI feature names]}` — `unsupported` is optional (added 29 Oct 2026); old clients ignore it |
| `eval` | `code` (string), `mode` (`code` \| `section` \| `file`), `path` (absolute; required for `file`, optional source path otherwise) | `{}` then events |
| `input_reply` | `text` (one line, no newline) | `{}` |
| `workspace` | — | `{variables:[{name,class,size:[r,c,...],bytes,complex,global,preview}]}` (sorted by name, at most 500) |
| `inspect` | `name`, `row`=1, `column`=1, `rows`=100, `columns`=30 | variable page, see below |
| `rename` | `old_name`, `new_name` | `{old_name,new_name}` |
| `clear` | `names` (array) or absent for all | `{names:[cleared names]}` |
| `cd` | `path` (absolute or relative to the engine folder) | `{cwd}` |
| `pwd` | — | `{cwd}` |
| `reset` | — | `{cwd}`: workspace emptied, figures closed, folder back to the start folder |
| `close` | — | `{}`; then the child exits with status 0 |
| `save` | `path`, `names` (array) or absent for all | `{path, names, warnings}` — see MAT files |
| `load` | `path`, `inspect`=false, `inventory`?, `replacements`? | inspect: `{path, variables, replacements, warnings}`; else `{path, names, replacements, warnings}` |

`file` mode: when `code` is absent or empty, the child reads `path` (UTF-8, at
most 2,000,000 bytes) and runs it as a script; errors name that file.

**Variable page** (`inspect`), the same shape the Variables editor reads:

```json
{"name":"x","class":"double","size":[1,1],"real":true,"kind":"matrix",
 "row":1,"column":1,"row_count":1,"column_count":1,"row_total":1,"column_total":1,
 "truncated":false,"editable":false,"rows":[["3"]]}
```

`kind` is `matrix` (with `rows`, strings at full precision), `char-text` (with
`text`) or `detail` (with `text`). `editable` is `false` until `variable_write`
exists.

A sparse matrix gets a `matrix` page read from its storage (any size, no full
form), with `"sparse":true` and `nnz` added. Its workspace row has the same
`class` as the full form (`double` or `logical`); the `preview` says `sparse`
(`sparse [3 0;0 4]`, or `100000x100000 sparse double` when it is not small).

**MAT files** (`save`, `load`; added 30 Oct 2026, real engine only — the
stub keeps answering `unsupported`, and `info` then lists `save`/`load` in
`ops` and no `mat_files` in `unsupported`). `path` is absolute or relative to
the engine folder.

- `save` writes MAT version 7 with the engine's `save`. `names` absent or
  `null` saves every variable (`invalid` when there is none); otherwise every
  name must exist (`invalid`). Function handles and string arrays are stored.
  A variable the engine cannot store (a table, a classdef object, a handle to
  a nested function) is skipped; inside a cell or struct that element is saved
  as `[]` and the rest is kept. Each is one entry of `warnings`, an element
  named by its path (`s.f{2}`). `names` lists the variables the written file
  holds.
- `load` with `inspect:true` reads the file without changing anything:
  `variables` are `{name, size:"3x3", class, bytes}` in file order (skipped
  values included, with their stored class), `replacements` the workspace
  variables a load would overwrite. Without `inspect` it assigns the file's
  variables in the base workspace; when `inventory` (the inspected
  `variables`) is given, the file's names, classes and sizes must still
  match it, and when `replacements` is given it must equal the current
  overwrite set; otherwise the answer is `invalid` and nothing is assigned.
- Errors: missing file `not_found`; not a MAT file, too large (256 MiB), or
  a failed check `invalid`; the engine failing `engine`.

`debug`, `breakpoint` and `run_to_cursor` below are implemented by the real
engine (see Debugger). **Defined but not implemented by the stub** — the child answers
`ok:false, code:"unsupported"` with a user-facing message and the UI feature:

| op | feature |
|---|---|
| `debug` (continue/step/in/out/quit/eval/inspect/frame) | `debugger` |
| `breakpoint` | `breakpoints` |
| `run_to_cursor` | `run_to_cursor` |
| `package` | `packages` |
| `profile` | `profiler` |
| `publish` | `publish` |
| `save` / `load` (MAT files; the real engine implements them) | `mat_files` |
| `assign_scalar` | `workspace_edit` |
| `variable_write` | `variable_editor_write` |
| `figure_rotate` | `figure_rotate` |

An engine that implements one of these lists it in `info.ops`; its parameters
are then specified here first.

## Interrupt

The client interrupts by sending **SIGINT to the child process**. Semantics:

- During `eval` the engine stops at its next check (between statements, inside
  `pause`, while waiting for input) and ends the eval with an `error` event,
  `interrupted:true`, message `Operation interrupted.`. A pending
  `input_request` is cancelled.
- Idle: no effect.
- Idempotent: several signals before the eval ends act as one. A signal never
  ends the child and never affects the next eval.
- Variables assigned before the interrupt stay.

The client should keep a time limit after an interrupt (the Python client
waits 5 s), then kill the child and report that the session must be reset.

## While an eval runs

The child keeps reading stdin. `input_reply` is answered when input is
pending (`invalid` otherwise), `info` is answered, `close` abandons the eval
(no terminal event), answers and exits; every other request gets `busy`.

## Debugger (added 30 Oct 2026)

`InterpEngine` implements `debug`, `breakpoint` and `run_to_cursor`; it lists
them in `info.ops` and no longer reports `debugger`, `breakpoints` or
`run_to_cursor` in `info.unsupported`. `StubEngine` still answers them with
`unsupported`. Everything here is additive: a client that never sets a
breakpoint and never runs `keyboard` sees the protocol above unchanged.

**Breakpoints** belong to the session, not to one eval: they survive evals
and `reset`, and the `dbstop`/`dbclear`/`dbstatus` builtins work on the same
set. A breakpoint is a file (absolute path of a `.m` file; the engine
canonicalises it) and a requested line. The line it actually stops at is the
first line at or after the requested one that starts a statement (MATLAB
rule: blank lines, comments and `function`/`end` lines snap forward), computed
against the file's current text each time the file is loaded.

| `breakpoint` `action` | parameters | result |
|---|---|---|
| `set` | `path`, `line` (≥ 1), `condition` (string, optional; empty = unconditional), `enabled` (bool, default `true`) | `{path, line, actual_line}`; replaces a breakpoint at the same path and line. `not_found` when the file does not exist, `invalid` when no statement starts at or after `line` |
| `clear` | `path`, `line` | `{path, line, cleared}` (`cleared` false when there was none) |
| `clear_all` | `path` (optional: only that file) | `{cleared}` (a count) |
| `list` | — | `{breakpoints:[{path, line, actual_line, enabled, condition}], stop_on_error}`; `actual_line` is `null` when the file cannot be read |

`breakpoint` is accepted when idle and while paused, not while code runs.

**Pause.** Code pauses at a statement boundary when it reaches an enabled
breakpoint whose condition is empty or true, when a step completes, at the
run-to-cursor line, at `keyboard`, and at an error not caught by `try` when
`dbstop if error` is on. A condition is evaluated in the paused frame; when
it raises an error the message `Error evaluating breakpoint condition: <msg>`
goes to `stderr` and the code pauses there anyway (Octave behaviour). Several
statements on one line pause once. Code typed in the Command Window (`eval`
mode `code`) and `section` text are not files: they never pause on their own
lines, but functions and scripts they call do. The whole call stack stays
alive while paused. The child then writes one event:

```json
{"type":"event","id":9,"event":"paused","reason":"breakpoint","file":"/w/f.m","line":10,"frame":1,
 "stack":[{"index":1,"name":"inner","file":"/w/f.m","line":10,"column":5,"current":true},
          {"index":2,"name":"middle","file":"/w/f.m","line":6,"column":5,"current":false}],
 "variables":[...same rows as the workspace op...],"run_to_cursor":null}
```

- `reason`: `breakpoint` | `step` | `keyboard` | `error` | `run_to_cursor`.
- `stack`: newest first, `index` from 1. A script called from a function or
  script is its own entry (MATLAB `dbstack`). The Command Window's base
  workspace is not an entry; a pause in code that is not a file (e.g.
  `keyboard` typed in the Command Window) has an empty `stack`, `file` `""`
  and `line` 0. `current` marks the selected frame; `frame` is its index
  (0 when the stack is empty), `file`/`line` are those of the selected frame.
- `variables`: the selected frame's variables (same shape as `workspace`).
- `run_to_cursor`: `{path, line}` (the actual line) while a run-to-cursor
  target is pending, else `null`.

The event is repeated (with the new selection, variables and stack) after
every `debug` `eval` and `frame` request. Between the `paused` event and the
next resume the child accepts `debug`, `breakpoint`, `run_to_cursor`,
`workspace` and `inspect` (both for the selected frame), `info`, `close`, and
answers `input_reply` with `invalid`; any other op gets `busy`. Outside a
pause `debug` and `run_to_cursor` get `busy` while code runs and `invalid`
when idle.

| `debug` `command` | parameters | result, then |
|---|---|---|
| `continue` | — | `{}`; the code runs on |
| `step` | — | `{}`; pauses at the next line of the top frame or of a caller (step over) |
| `in` | — | `{}`; pauses at the next line anywhere (step in) |
| `out` | — | `{}`; pauses at the next line of a caller (step out) |
| `quit` | — | `{}`; the eval ends with an `error` event, `interrupted:true`, `quit:true`, message `Debugging ended.` |
| `eval` | `code` (string, one line) | `{}` after the code ran in the selected frame (output as `output` events, an error as `error: <msg>` on `stderr`); then a new `paused` event. `dbcont`, `dbstep [in\|out]`, `dbquit`, `dbup`, `dbdown` in that code act as the commands above. Breakpoints do not fire inside it |
| `frame` | `frame` (integer, a stack `index`) | `{}`, then a new `paused` event; `invalid` for an index not in the stack |
| `inspect` | `name`, `row`, `column`, `rows`, `columns` | a variable page (as `inspect`) of the selected frame |

| op | parameters | result |
|---|---|---|
| `run_to_cursor` | `path`, `line` (only while paused) | `{path, line, actual_line}`; sets a temporary breakpoint and continues. The temporary breakpoint is removed when it is reached, when the eval ends and on `quit`; it stays pending (and is reported in `paused`) when another breakpoint pauses first. `invalid` while one is already pending |

**Interrupt while paused** (SIGINT) ends the eval like an interrupt while
running: `error` event, `interrupted:true`, message `Operation interrupted.`
Several signals act as one; one sent between a resume and the next pause is
seen at the next statement. Variables assigned before stay.

`dbstop`/`dbclear`/`dbstatus` builtins: `dbstop in NAME [at LINE] [if
COND]`, `dbstop NAME LINE`, `dbstop('NAME', LINE)`, `dbstop if error`;
`dbclear all`, `dbclear in NAME [at LINE]`, `dbclear NAME [LINE]`, `dbclear if
error`; `dbstatus` (prints, or returns an N×1 struct with `name`, `file`,
`line` (actual lines), `cond` (a cell of conditions)). `dbcont`, `dbstep [in|out]`, `dbquit`, `dbup
[N]`, `dbdown [N]` work only while paused (`debug` `eval`); elsewhere they are
an error. Without a session (the `indy` CLI) nothing ever pauses and
`keyboard` does nothing.

## Rust side

`motor/crates/indy-kernel`:

```rust
pub trait Engine {
    fn info(&self) -> EngineInfo;
    fn eval(&mut self, request: &EvalRequest, host: &mut dyn Host) -> Result<(), EvalError>;
    fn workspace(&self) -> Vec<VariableSummary>;
    fn inspect(&self, name: &str, page: PageRequest) -> Result<Json, EngineError>;
    fn cwd(&self) -> PathBuf;
    fn cd(&mut self, path: &Path) -> Result<PathBuf, EngineError>;
    fn reset(&mut self);
    fn rename(&mut self, old: &str, new: &str) -> Result<(), EngineError>;   // default: unsupported
    fn clear(&mut self, names: Option<&[String]>) -> Result<Vec<String>, EngineError>; // default: unsupported
    fn debugger(&self) -> bool;                                          // default: false
    fn breakpoint(&mut self, body: &Json) -> Result<Json, EngineError>;  // default: unsupported
}

pub trait Host {
    fn output(&mut self, stream: Stream, text: &str);
    fn clc(&mut self);
    fn figure(&mut self, figure: FigureEvent);
    fn input(&mut self, prompt: &str) -> Result<String, Interrupted>;
    fn interrupted(&mut self) -> bool;
    fn paused(&mut self, fields: Vec<(&'static str, Json)>);           // default: nothing
    fn pause_wait(&mut self) -> PauseRequest;                          // default: Unsupported (go on)
    fn pause_reply(&mut self, id: u64, result: Result<Json, EngineError>);
}
```

A pause runs inside the interpreter's statement hook (`indy_interp::debug`,
`DebugController`), so the whole call stack stays alive; `InterpEngine` installs
`debugger::DebugBridge` for each eval, which serves requests through these
three `Host` methods.

The engine never writes to stdout; everything goes through `Host`. The engine
calls `host.interrupted()` between statements and inside long operations.
`server::serve` speaks this protocol for any `Engine`; the `indy-kernel`
binary is `serve` + an engine + a SIGINT handler. The engine is `InterpEngine`
(the real interpreter, `indy-interp`) by default; `--stub` or
`INDYMAT_R_STUB=1` select `StubEngine`.

## Python side mapping (`backend/kernel_rust.py`)

| `Kernel` method / mode | protocol |
|---|---|
| `submit(code, 'code')`, `/api/folder` (`cd('…');`) | `eval` mode `code` |
| `submit(mode='file', argument=path)` | `eval` mode `file`, `path` |
| `submit(mode='inspect', argument=name)` | `inspect` → `state.detail` |
| `workspace_action('variable-read', …)` | `inspect` → `state.variable_action` |
| `workspace_action('workspace-rename' / 'workspace-clear-names', …)` | `rename` / `clear` |
| `input(text)` | `input_reply` |
| `interrupt()` | SIGINT |
| `reset()` / `close()` | `reset`, else restart / `close` |
| `workspace_action('workspace-save' / 'workspace-load-inspect' / 'workspace-load', …)` | `save` / `load` `inspect:true` / `load` → `state.workspace_action` |
| `debug`, `configure_breakpoint`/`clear_breakpoints`/`relocate_file_breakpoints`, `run_to_cursor` | `debug`, `breakpoint` (one job; disabled breakpoints stay in the Python map; a line without code moves the map entry to `actual_line`), `run_to_cursor`; `paused` → state `status:'paused'`, `debug{file,line,frame,stack,ready,serial,reason}`, `variables`, `run_to_cursor` |
| packages, profile, publish, scalar edit, cell write, rotate | the `unsupported` op; its message is raised as `ValueError` (HTTP 400, shown by the UI) |
| state `variables` | `workspace` after every job |
| state `version` | `engine.description` |
