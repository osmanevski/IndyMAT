#!/usr/bin/env node
// Validates indy-graphics output with the UI's own admission gate,
// frontend/figure_data_utils.cjs `validate()`: the only gate that decides
// whether a figure is drawable. Usage:
//   node motor/scripts/grafik_validate.cjs <dir-with-json> [expectations.json]
// Each <name>.json must give ok:true. With an expectations file
// ({name: "supported" | "<reason_code>"}), supported/reason_code must match.
"use strict";
const fs = require("fs");
const path = require("path");

const repo = path.resolve(__dirname, "..", "..");
const { validate } = require(path.join(repo, "frontend", "figure_data_utils.cjs"));

const dir = process.argv[2];
if (!dir) {
  console.error("usage: grafik_validate.cjs <dir> [expectations.json]");
  process.exit(2);
}
const expect = process.argv[3] ? JSON.parse(fs.readFileSync(process.argv[3], "utf8")) : {};
const files = fs.readdirSync(dir).filter((f) => f.endsWith(".json")).sort();
let bad = 0;
let drawable = 0;
for (const file of files) {
  const name = file.slice(0, -5);
  const text = fs.readFileSync(path.join(dir, file), "utf8");
  let payload;
  try {
    payload = JSON.parse(text);
  } catch (e) {
    console.log(`FAIL ${name}: not JSON (${e.message})`);
    bad++;
    continue;
  }
  const r = validate(payload);
  const state = r.ok ? (r.supported ? "supported" : r.reason_code) : `INVALID ${r.reason_code} at ${r.path}`;
  const want = expect[name];
  const ok = r.ok && (want === undefined || want === state);
  if (r.ok && r.supported) drawable++;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}: v${payload.version} ${state}${want !== undefined && want !== state ? ` (expected ${want})` : ""}`);
  if (!ok) bad++;
}
console.log(`${files.length} figures, ${drawable} drawable, ${files.length - drawable} explicit fallback, ${bad} failing`);
process.exit(bad ? 1 : 0);
