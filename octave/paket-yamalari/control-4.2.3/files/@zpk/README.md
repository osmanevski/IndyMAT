# IndyMAT zpk representation

This is a genuine Octave old-style `zpk` value class derived from the installed
`lti`, holding ordered zero and pole column vectors, scalar gain and public
metadata. It is not a renamed `tf`. It does not claim a MATLAB `DynamicSystem`
superclass. The required `string` type comes from the already installed datatypes
package; no dependency is added.

The supported numerical models have one input and one output, continuous time
with `Ts=0`, and zero delays. Empty default models are supported for storage and
inspection. Direct numeric or 1-by-1 cell root inputs preserve order, repeated
roots and cancellations; zero gain retains roots. Real or complex finite roots
and gain are stored as double. Static scalar gains, single precision inputs,
`zpk('s')`, identity conversion, and SISO CT `tf`/`ss` conversion are supported.

`properties`, struct and case-insensitive property `get`, paired property `set`,
no-output caller mutation, dot/nested root assignment, value copy independence,
and scalar/colon SISO selection are supported. Public defaults follow the
measured 20-property record. Channel names/units are scalar text cells, Notes
uses a string column, TimeUnit supports seconds, groups contain SISO index 1,
SamplingGrid supports only an empty struct, and DisplayFormat supports roots
and frequency. Unknown properties and unimplemented query forms reject.

The numerical subset includes stored `pole`/`zero`, explicit-frequency
`freqresp` and output `bode`, `dcgain`, `isstable`, numerical output `step`,
`+`, `-`, `*`, `/`, `feedback`, `series`, `parallel` and inverse. Ordered mixed
operations with tf/scalar/PID produce zpk; existing ss precedence produces ss.
Multiplication, division and series concatenate actual stored root vectors in
operand order. Sum/difference retain concatenated poles and compute numerator
roots from actual transfer arithmetic. Feedback computes the actual closed-loop
transfer function. No fixture roots or responses are manufactured.

Non-conjugate roots and complex gains have direct complex frequency responses.
The installed Octave tf representation accepts real polynomial coefficients
only. Such a model therefore rejects tf/ss conversion, and operations requiring
that conversion, with the explicit limitation below. Complex data is never
projected to real to conceal the restriction.

Newly computed roots can have a different order from MATLAB, particularly sums,
differences and feedback. MATLAB's state-space subtraction may retain tiny
floating residuals where the Octave transfer-polynomial calculation returns
exact zero. Numerical transfer equivalence and result class do not establish
ordered root parity. The `zpk(ss(...))` conversion calls actual `zero` and `pole`
algorithms, rather than sorting fixture roots. Octave's automatic step time grid
is retained; default-grid MATLAB parity is not claimed. Plotting, MIMO,
discrete-time models, nonzero delays, model arrays, arbitrary channel indexing,
unmeasured options and arbitrary PID fitting are outside this subset.

## Evidence

Behavior is grounded in the real MATLAB R2025b artifact
`olc26-control.txt` (wave 26), with these independently guarded rows:

- Z01–Z21 and D01–D21: constructors, shapes, storage, zero gain and complex data.
- ZP01–ZP18/ZP21: public properties and getter forms; ZP19/ZP20: rejected query forms.
- ZS01–ZS12, ZA01–ZA10, ZI01–ZI03: mutation, copies and SISO selection.
- CV01–CV08: tf/ss/identity conversions and actual root recomputation.
- AN01–AN11: numerical analysis, stability/gain boundaries and step outputs.
- ADD/SUB/MUL/DIV/FB/SER/PAR with ordered Z/T/S/N suffixes, INV_Z and FB_POS:
  result classes and transfer values. PADD/PSUB/PMUL/PDIV/PFB/PSER/PPAR_ZL/ZR:
  mixed PID/zpk classes and transfer values.
- LIMIT01–LIMIT25: supported and excluded form boundaries. Project scope errors
  below are explicit local contracts, not claims of identical MATLAB errors.

`tests/test_control_zpk26.py` installs the versioned additions only through
`scripts/patch_packages.py` into a private copied package and uses cold,
nongraphics `octave-cli` processes. Tests distinguish exact stored order from
numeric transfer tolerance, and verify existing tf/ss result classes.

## Local errors

| Identifier | Exact message |
|---|---|
| `IndyMAT:control:UnsupportedMIMO` | This representation supports SISO models only. |
| `IndyMAT:control:UnsupportedDiscreteTime` | This representation supports continuous-time models with Ts equal to zero only. |
| `IndyMAT:control:UnsupportedDelay` | This representation does not support time delays. |
| `IndyMAT:control:UnsupportedModelArray` | This representation does not support arrays of models. |
| `IndyMAT:control:UnsupportedCallForm` | This call form is outside the documented control representation subset. |
| `IndyMAT:control:UnsupportedProperty` | This property is outside the documented control representation subset. |
| `IndyMAT:control:UnsupportedComplexConversion` | The installed Octave tf representation supports real polynomial coefficients only. |
| `IndyMAT:control:InvalidRoots` | Roots must be finite numeric vectors. |
| `IndyMAT:control:InvalidGain` | Gain must be a finite numeric scalar. |

Errors raised by the numerical engine itself are not caught and reclassified.

Round7 R2025b inc2 rows: `set(zpk(),'Z',{-1},'P',{-2},'K',3)` populates a SISO model, including parent dimensions. Public mutations validate compatible Z/P/K sizes after the complete batch. Inconsistent partial mutation raises `Control:ltiobject:zpkProperties4`: `The values of the "Z", "P" and "K" properties must have compatible sizes.` Stored root/gain conversion already uses double.
