"use strict";
// Scalar alpha is the documented Octave subset. Data descriptors stay rejected
// until their mapping/association contract has an explicit reference decision.
const unit = value => Number.isFinite(value) && value >= 0 && value <= 1;
function validateAlphaDescriptor(input, count, association) {
  if (input === undefined) return { ok: true };
  if (input && input.mode === "constant" && input.association === "constant" && unit(input.value)) return { ok: true };
  return { ok: false, reason_code: "transparency", reason_args: { property: "alpha", detail: "unmeasured_data_mapping" } };
}
function alphaAt(descriptor, index) {
  if (descriptor === undefined) return 1;
  if (descriptor.mode === "constant" && unit(descriptor.value)) return descriptor.value;
  throw new Error("Unvalidated alpha descriptor");
}
module.exports = { validateAlphaDescriptor, alphaAt };
