# IndyMAT block models, schema 1 (experimental)

This is a narrow Octave netlist layer, **not Simulink compatibility**. No
`.slx`/`.mdl` interpreter, model callbacks, or generated code is provided. The
IndyMAT block diagram editor (see "Diagram files" below) edits these models.
Version 1 extends the feasibility study's v0 with exactly two blocks authorized
on 13 October 2026: discrete `SampledInput` and `ClampingGate`. There is no PID
mask or DiscreteFilter block. FIR/IIR and Euler integrals are ordinary networks
of Gain, Sum and UnitDelay. Existing Octave is the only runtime.

## API and results

```octave
model = im_example_model('ayrik_kontrol', struct('Ts', 0.05));
plan = im_compile(model);             % validation and inspectable execution plan
result = im_simulate(model);          % validates again; never accepts a plan
t = result.t; y = result.signals.output;
stairs(t, y);                        % optional ordinary Octave figure
result2 = im_simulate(jsonencode(model));
% For a user-owned JSON text file:
% result3 = im_simulate(fileread('model.imb.json'));
```

`im_compile` and `im_simulate` take one scalar struct or JSON **text**, never a
filename. The same numeric semantics apply to both. JSON duplicate keys
(including escaped-equivalent names) are rejected; keys are never renamed.
Parameters contain numbers, not workspace expressions, function handles, or
code. No parameter is evaluated. These helpers do not read/write model files;
user code/file operations retain their existing trust and atomic/hash-save
rules. No new server route, file permission bypass, or automatic overwrite exists.

`result.t` is a double column vector. `result.signals.<ScopeID>` is an ordinary
double matrix: rows are times, columns are Mux input order. Only Scopes log.
`result.states` has rows=time and columns=state, including initial and final
states; columns follow block declaration order and each block's `plan.blocks{k}.state`
indices. A stateless model returns an N-by-0 state matrix. Helpers create no
figures, publish nothing, and change no base variables, RNG, global solver
options, paths, packages or persistent state. Assignment of a result is explicit;
an error/Stop does not assign a partial result. Use plot/stairs after completion
through the normal job figure path; rendering is separate from simulation.

Plans expose `model`, normalized `blocks`, integer topological `order`,
`simulation`, `t`, and `x0`. Plans are diagnostics, not a serialized/stable public
execution format. Editing a plan cannot bypass validation.

## Exact data model

All listed fields are required unless explicitly optional. Unknown fields are
errors, including layout, reset, callbacks and unsupported solver options.
Numeric values must be full, real, finite **double** values; no implicit
single/integer/logical/complex conversion or scalar expansion is performed.

- Root: `schemaVersion=1`, `blocks`, `connections`, `simulation`.
- `blocks`: nonempty cell vector of scalar structs or struct vector. Order does
  not determine signal execution order. Row and column vectors are accepted.
- Block: `id`, `type`, `params`, `sampleTime`. IDs match
  `[A-Za-z][A-Za-z0-9_]{0,62}` and are unique/case-sensitive.
  `initialState` is required only for Integrator/UnitDelay, forbidden otherwise.
- `connections`: cell/struct vector, or empty `[]`. Each connection has
  `from=struct('block',id,'port',1)` and `to=struct('block',id,'port',n)`.
  Ports are positive integer doubles, numbered from 1. Each input needs exactly
  one driver. Fan-out and unused outputs are allowed; unused blocks are still
  validated/evaluated. Scope has no output; all other types have one output port.
- Every block has explicit `sampleTime`: **0** in continuous mode, or exactly
  the model's **Ts** in discrete mode. No inherited rates, offsets or rate transitions.

Example JSON (all keys and options are intentionally explicit):

```json
{
  "schemaVersion": 1,
  "blocks": [
    {"id":"u","type":"Step","params":{"time":0.375,"before":0,"after":1},"sampleTime":0},
    {"id":"x","type":"Integrator","params":{},"initialState":0,"sampleTime":0},
    {"id":"output","type":"Scope","params":{},"sampleTime":0}
  ],
  "connections": [
    {"from":{"block":"u","port":1},"to":{"block":"x","port":1}},
    {"from":{"block":"x","port":1},"to":{"block":"output","port":1}}
  ],
  "simulation":{"mode":"continuous","t0":0,"tf":1,"solver":"ode45","outputDt":0.1,"relTol":1e-9,"absTol":1e-11,"maxStep":0.02}
}
```

## Blocks and parameters

| Type | Exact params | Inputs and meaning |
|---|---|---|
| Constant | `value` | No input; scalar value |
| Step | `time,before,after` | No input; before at t<time, after at t>=time |
| Sine | `amplitude,omega,phase,bias` | No input; amplitude*sin(omega*t+phase)+bias; omega rad/s, absolute t |
| Gain | `gain` | 1 scalar input; gain*u |
| Sum | `signs` | Nonempty vector of +1/-1; one scalar input per sign, ordered sum |
| Saturation | `lower,upper` | 1 input; min(upper,max(lower,u)); lower<=upper |
| Integrator | empty struct | Continuous only; x'=u, y=x; finite scalar initialState |
| TransferFcn | `num,den` | Continuous only; strictly proper SISO descending-power coefficient vectors; zero initial state |
| Mux | `inputs` | Positive integer scalar; packs that many scalars for observation only |
| Scope | empty struct | 1 input, scalar or Mux vector; logged under its id; no output |
| UnitDelay | empty struct | Discrete only; y[k]=q[k], q[k+1]=u[k]; finite scalar initialState |
| SampledInput | `values` | Discrete only; finite numeric vector of exactly N grid values; at step k returns values(k+1) |
| ClampingGate | `lower,upper` | Discrete only; port1 error e, port2 raw control v; lower<=upper |

ClampingGate returns zero iff `(v>upper && e>0) || (v<lower && e<0)`, else e.
**At either exact limit it returns e**, so an attached Euler integral still
updates. It has no state. Feedback must pass through UnitDelay; this is not
a continuous anti-windup solver or resettable integrator.

TransferFcn requires nonzero leading coefficients (scalar zero numerator is
allowed), `length(num)<length(den)`, denominator degree 1..64. No improper,
equal-degree feedthrough, ideal derivative, pole cancellation, or user TF state
initialization. Normalize by den(1); the compiler constructs a controllable
companion realization with `A=[-den(2:end); eye(n-1),zeros(n-1,1)]`,
`B=[1;zeros(n-1,1)]`, `C=[zeros(1,n-length(num)),num]`. These states are this
implementation's coordinates, not a claim about another tool's internal states.

Mux may drive only Scope, even if it has one channel. It cannot drive another
Mux or scalar math/state blocks. No bus, MIMO, variable size or matrix algebra.
SampledInput never interpolates, extrapolates, repeats or generates noise. Its
record spans t0 through tf inclusive with precisely the grid length.

## Time, scheduling and numerical limits

Continuous simulation fields: `mode='continuous',t0,tf,solver='ode45',outputDt,
relTol,absTol,maxStep`. Discrete fields: `mode='discrete',t0,tf,solver='discrete',Ts`.
There are no implicit defaults. t0<tf; intervals and tolerances are positive.
Duration/interval must be an integer within **32 ulps of that ratio** to allow
floating point representation; no shortened trailing interval is added. The
grid is t0+(0:N)'*interval, with the final value set to tf. Collapsed/nonfinite
grids are errors. Discrete sample coverage is checked by exact vector length.

Continuous states use Octave ode45 (non-stiff ODE). Known Step times inside
the interval split integration into separate solves. The RHS at a segment's
right endpoint still uses that segment's left-side Step value; the next segment
and logged samples use the right-continuous value. Sine retains actual absolute
time. All requested output times plus boundaries are supplied to ode45; exact
requested rows are selected, including Octave's two-point-tspan case. No
interpolation crosses a Step. RHS evaluations are pure: rejected adaptive steps
cannot advance discrete state or append logs. No global solver settings change.
Saturation is continuous but nonsmooth; no state event/zero-crossing claim is made.

Discrete simulation computes every output from the **same old states**, logs it,
then commits every UnitDelay input together. The final sample is logged without
an additional state update. A continuous plant can be manually discretized first
(the example uses its exact ZOH recurrence); that is not a mixed scheduler.

Integrator, strictly proper TransferFcn and UnitDelay outputs cut instantaneous
dependencies. All other blocks use their instantaneous inputs. Topological
sorting rejects every direct-feedthrough cycle, including disconnected ones,
with block/port cycle edges. No implicit delay or algebraic equation solve is
inserted. Mixed continuous/discrete, multiple rates, stiff/DAE solvers, reset,
triggered/enabled subsystems, event priorities, PID masks, external callbacks,
transport delays, model references and file imports are unsupported.

Budgets: 4,194,304 JSON characters, 256 blocks, 4096 connections, 256 inputs per
Sum/Mux, 100001 output times, and 5,000,000 doubles total for logged signals plus
state history. Nonfinite intermediate signals, derivatives or states fail even
when downstream saturation could make an output finite. Tolerances bound solver
error estimates, not a promise for every ill-conditioned/stiff model. Stop uses
the existing Kernel interrupt; solver failures/interrupts are not retried.

## Exact error identifiers

Identifiers are stable; messages include the offending field, block or edge.
Representative exact templates (placeholders are formatted with actual values):

| Identifier | Rejected input / exact message example |
|---|---|
| `im:model:JSON` | `Invalid JSON text.` / `Duplicate JSON object key: %s.` |
| `im:model:Schema` | `schemaVersion must be the double scalar 1.` / `%s has unsupported field %s.` |
| `im:model:Type` | `Block %s: unsupported type %s.` |
| `im:model:Parameter` | `Block %s parameter %s must be a finite real double scalar.` / `Block %s SampledInput requires exactly %d values for the simulation grid.` |
| `im:model:State` | `Block %s requires a finite real double scalar initialState.` / `Block %s does not accept initialState; TransferFcn initial states are zero.` |
| `im:model:Mode` | `Unsupported simulation mode; only continuous and single-rate discrete are supported.` / `Block %s (%s) is not supported in %s mode; mixed models are unsupported.` |
| `im:model:SampleTime` | `Block %s sampleTime must equal %.17g; inherited and multiple rates are unsupported.` |
| `im:model:Solver` | `Mode %s requires solver %s; other solvers are unsupported.` / `ode45 did not cover the complete time segment.` |
| `im:model:Time` | `Simulation duration must be an integer multiple of outputDt or Ts (32-ulp ratio tolerance).` |
| `im:model:Limit` | `Model exceeds 256 blocks or 4096 connections.` / `Logged signals plus states exceed 5000000 output doubles.` |
| `im:model:Connection` | `Block %s input %d has no driver.` / `Block %s input %d has more than one driver.` |
| `im:model:Port` | `Connection %d: port out of range (%s.out%d -> %s.in%d).` |
| `im:model:Dimension` | `Mux %s may connect only to a Scope input.` |
| `im:model:AlgebraicLoop` | `Algebraic loop: %s. Add no implicit delay; this model is unsupported.`; path lists actual `.out1 -> .inN` edges |
| `im:model:Nonfinite` | `Nonfinite or nonreal value in %s.` |

An unknown block such as TriggeredSubsystem fails Type; a reset parameter fails
Parameter/Schema; a mixed mode fails Mode; different rates fail SampleTime;
`.slx` text is invalid JSON. A schema-valid model cannot request these operations.
Structural errors in connections/endpoints use Connection, in parameters use
Parameter, and in root/block/simulation use Schema. Compiler source contains
the complete formatted messages. Internal Octave errors and interrupts retain
their own information; known adaptive solver exhaustion maps to Solver.

## Diagram files (`.imbd.json`) and the editor

The IndyMAT block diagram editor keeps one diagram per file. A file whose name
ends in `.imbd.json` opens from the Current Folder as a diagram. The file wraps
an unchanged schema-1 model with the editor's block positions:

```json
{"imbVersion":1,
 "model":{"schemaVersion":1,"blocks":[...],"connections":[...],"simulation":{...}},
 "layout":{"blocks":{"u":{"x":20,"y":170},"x":{"x":190,"y":170},"output":{"x":360,"y":170}}}}
```

- `imbVersion` is 1. `model` is exactly the schema-1 model of this document, so
  `im_compile`/`im_simulate` accept it unchanged. `layout.blocks` has exactly one
  `{x,y}` entry per block id (finite numbers, |value| <= 1e6). Unknown fields at
  any level and duplicate JSON keys are errors; nothing else is stored.
- The editor checks structure when it opens a file (fields, types, block types,
  exact parameter names, numbers, port numbers that exist). A file that fails
  opens as text with the reason; "Open as Diagram" retries after a repair.
  Value rules stay in `im_compile` and appear at Run: signs of +/-1, strict
  properness, SampledInput length, drivers, algebraic loops, the time grid. An
  incomplete diagram can be edited and saved.
- Parameters are typed as plain decimal numbers; nothing is evaluated. The
  property panel applies the rules of the table above where it can. `sampleTime`
  follows the simulation: 0 in continuous mode, `Ts` in discrete mode.
- The file is ordinary text in the editor tab: dirty state, drafts, the close
  prompt, Save As (the name must end in `.imbd.json`) and the atomic save with the
  hash conflict check are the text editor's. The editor writes one block,
  connection or position per line.
- Run saves first and then sends ordinary, visible Octave code through the
  normal execute path (no new route): `<stem>_result = (@im_simulate)('<model
  JSON>')`, with the model as compact single-line ASCII JSON in a text literal.
  `<stem>` is the file name without `.imbd.json` when it is a valid identifier of
  at most 56 characters, otherwise the variable is `diagram_result`. No other
  base variable is written. Each Scope gets one figure named `<file name>: <Scope
  id>`; the code first closes only figures with exactly that name, then plots
  (`plot` in continuous mode, `stairs` in discrete mode; a Mux-fed Scope has a
  legend of the Mux sources). Stop, job identity and the console are those of any
  command. When an `im_compile` message names a block or connection, the editor
  selects it.

## Examples and acceptance

`im_example_model(name,options)` provides the five stage-0 equation networks.
Names are original script stems. PID accepts `gains=[Kp Ki Kd]` (default [2 1 1]);
mass accepts no options; discrete control/anti-windup accept `Ts` (defaults .05/.002).
Filtering requires `values,b_fir,a_fir,b_iir,a_iir,Ts` from the original script's
same-session record. Unknown example/options fail `im:example:Name` or
`im:example:Parameters`. Factories do not compute output trajectories.

The five `workspace/examples/netlist_*.m` scripts show ordinary result variables
and optional plot/stairs. Run `sinyal_suzme.m` first for the filter input; the
netlist example does not regenerate its RNG or redesign its coefficients.
Examples-window registration is a separate integration edit, not automatic.

Run `python3 -m unittest discover -s tests -p 'test_netlist_*.py' -v` for new
server-free tests. `netlist_unit.m` checks all blocks, validation and scheduling;
`netlist_numerical.m` runs the unchanged original numeric sections and compares
all five models, both sample periods/refinement steps, PID metrics, signal
spectra and original reference assertions. Numerical tolerances: PID 2e-7,
mass 1e-8, discrete/control/filter 1e-10, original anti-windup refinement .005
and area .01. Tests use real octave-cli, not numerical mocks or graphics.

MATLAB/Simulink, actual figure rendering, and browser presentation are separate
reference/integration checks. No cross-tool equivalence follows from the Octave
tests or the matching block names.
