// One English source must have one Turkish text across all locale parts (they are merged into one table).
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const dir = path.join(__dirname, "..", "frontend", "locales");
const seen = new Map();
const clashes = [];
for (const file of fs.readdirSync(dir).filter((name) => name.endsWith(".js")).sort()) {
  const source = fs.readFileSync(path.join(dir, file), "utf8").replace(/^import .*$/gm, "").replace("export default", "module.exports =");
  const module_ = { exports: {} };
  new Function("module", source)(module_);
  for (const [key, value] of Object.entries(module_.exports)) {
    assert(typeof value === "string" && value.length > 0, file + ": empty translation for " + key);
    if (seen.has(key) && seen.get(key).value !== value) clashes.push(key + ": " + seen.get(key).file + "=" + JSON.stringify(seen.get(key).value) + " vs " + file + "=" + JSON.stringify(value));
    else if (!seen.has(key)) seen.set(key, { file, value });
  }
}
assert.deepEqual(clashes, [], "conflicting translations:\n" + clashes.join("\n"));
console.log("I18N COLLISIONS PASS: " + seen.size + " sources, one Turkish text each.");
