# IndyMAT continuous-time PID representation

This is an old-style `pid` value class derived from the installed control
package's `lti`. It stores the actual finite real scalar `Kp`, `Ki`, `Kd`,
and nonnegative `Tf`; it does not rename a transfer function. The supported
model is SISO with `Ts=0`, no delays, and no arrays of models.

## Implemented forms and evidence

The source of MATLAB behavior is `olcum2/olc26-control.txt` in the wave-26
measurement record, parsed into `control26-rows.json`. These are measured
subset claims, not full Control System Toolbox compatibility.

| Supported behavior | MATLAB rows |
| --- | --- |
| `pid()` defaults to `(1,0,0,0)`; scalar one-to-four gain arguments, optional fifth zero sample time; zero branches and retained filter parameter | PID01–10 |
| Parallel realization `Kp + Ki/s + Kd*s/(Tf*s+1)`, with absent branches omitted; stored `Tf` is retained when `Kd=0` | PID01–10, PC01 |
| Negative filter, nonfinite/complex gains reject with the measured errors | PID11, PID14–15 |
| Twenty ordered public property names; scalar `get` struct and individual getters; default metadata includes `Notes` as installed datatypes `string` of size 0-by-1 | PP01–02 |
| `Kp`, `Ki`, `Kd`, `Tf`, `Name` mutation; independent copies, dot assignment and caller update for no-output `set` | PS01–04, PS14–15, PA01 |
| `IFormula`/`DFormula` accept `ForwardEuler`, `BackwardEuler`, `Trapezoidal`; CT getters remain empty and transfer values do not change | PS05–10 |
| Unsupported formula, set-query syntax and read-only delays use measured MATLAB errors | PS11, PP03, PS13 |
| `tf(pid)` and `ss(pid)`; inverse recognition of constant, PI, filtered PD and filtered PID transfer forms, including corresponding `ss` input | PC01–05, PC08, PC10 |
| Seven binary operations with PID, tf, ss, zpk and real scalar numbers; inverse | PADD, PSUB, PMUL, PDIV, PFB, PSER, PPAR ordered pairs, PINV |

The public metadata getters return measured defaults. Only `Name` is writable
metadata in this subset; input/output names, units, groups, notes, userdata,
time units and sampling grids are not writable here. Internal `get(...,'lti')`
returns the genuine parent representation. `set(...,'lti',...)` is rejected
rather than permitting replacement of the parent and a sample-time bypass.

For exact inverse recognition, transfer coefficients are normalized by their
leading denominator coefficient. Direct `tf` input must have one of the
supported denominator forms: a nonzero constant, `s`, `s+a`, or `s*(s+a)`
with `a>0`, and numerator degree within the corresponding measured form.
There is no arbitrary approximation or tuning. A direct nonzero constant
coefficient on the purported integrator denominator is not discarded.
For `ss` input only, denominator residuals below `64*eps*max(1,norm(d,inf))`
are cleared to account for the measured PC08 numerical reconstruction.
The scope excludes unfiltered PD/PID inverse conversions, other LTI classes,
time delays, MIMO systems and model arrays.

Result classes follow the measured ordered matrix: PID/PID gives `tf`;
PID/tf gives `tf`; PID/zpk gives `zpk`; PID/ss gives `ss`. Real scalar
addition, subtraction, multiplication, series, parallel and PID divided by
scalar preserve `pid`; scalar divided by PID and feedback give `tf`.
Native tf/ss algorithms perform transfer arithmetic and conversion.

## Project scope errors

These identifiers describe IndyMAT's bounded implementation. They do not
claim that MATLAB rejects a valid form outside this subset.

| Identifier | Exact message | Boundary |
| --- | --- | --- |
| `IndyMAT:control:UnsupportedMIMO` | `This representation supports SISO models only.` | nonscalar tf/ss conversion or PID-dispatched binary operand |
| `IndyMAT:control:UnsupportedDiscreteTime` | `This representation supports continuous-time models with Ts equal to zero only.` | nonzero constructor/set sample time, conversion, or PID-dispatched binary operand |
| `IndyMAT:control:UnsupportedModelArray` | `This representation does not support arrays of models.` | nonscalar gain values and nested/non-dot assignment |
| `IndyMAT:control:UnsupportedCallForm` | `This call form is outside the documented control representation subset.` | unsupported constructor/reference or numerical operand form |
| `IndyMAT:control:UnsupportedProperty` | `This property is outside the documented control representation subset.` | unknown/unimplemented properties, including parent replacement |
| `IndyMAT:control:UnsupportedPIDConversion` | `This transfer function is outside the supported exact PID conversion forms.` | transfer model not recognized as one of the exact supported forms |

Negative `Tf`, nonfinite/complex scalar gains, invalid formula strings,
`set(pid)` and delay writes have the measured `Control:...` identifiers
from PID11/14/15, PS11, PP03 and PS13. A MATLAB rejection row does not
justify borrowing its identifier for a different valid-but-unimplemented form.

## Native methods and limitations

The parent package remains responsible for numerical algorithms. Numeric
frequency response, poles, zeros, DC gain and stability dispatch through a
materialized transfer function; separate MATLAB analysis-method parity is
not claimed by the PID construction/conversion rows. Plotting, automatic
step grids, alternative method options, model concatenation, channel/model
indexing, discrete conversion and unmeasured methods are outside this
representation contract. Unsupported inherited calls can currently produce
native missing-hook or native argument errors rather than a project scope
identifier. No graphical or automatic-grid claim is made.

Versioned boundary patches guard the seven native `@lti` binary entry
points when either original operand is PID or ZPK. They validate CT and
SISO before inversion, conversion or static-gain sample-time promotion
can remove the new representation's identity. This covers `ss`-winning
operations in both operand orders, including static PID and division.
Native tf/ss-only operations do not enter these new checks.
`tests/test_control_boundary26.py` verifies rejection in both orders for
seven operations and preservation of normal CT/SISO results and native-only
discrete/MIMO operations.

`pidtune` is not implemented and no tuning stub is provided. TUNE01 produces
PI parameters even when `PID` is requested; TUNE02 produces an unfiltered
PID. Their crossover frequencies and phase margins differ. Two plant
fixtures cannot identify a general automatic-crossover or controller-
structure selection algorithm. Tuning needs a separate numerical algorithm
and measurement project covering plant families, options and stability.

`tests/test_control_pid26.py` runs real `octave-cli` against a private control
copy, installs these additions through the schema-2 package patcher, and
uses the installed datatypes implementation. It never changes the live
package tree. Tests distinguish measured MATLAB errors from project scope
rejections and compare numerical transfer responses as well as stored
values and result classes.

Round7 R2025b inc2 rows pin integer/single gain storage as double and scalar logical Kp rejection (`Control:ltiobject:pidSet1`). Validation normalizes numeric gains before transfer arithmetic; character input handling is unchanged (no new measured row).
