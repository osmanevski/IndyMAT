const assert = require("node:assert/strict");
const fs = require("node:fs");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const before = await (await fetch(base + "api/state", { headers: { "X-MF-Language": "tr", "X-MF-Token": token } })).json();
  const index = await (await fetch(base)).text();
  const script = await (await fetch(base + "app.js")).text();
  const style = await (await fetch(base + "style.css")).text();
  assert(index.includes('id="settings"'));
  assert(script.includes("mf-settings-v1"));
  assert(style.includes("Settings, persisted desktop layout and shortcut registry"));
  assert.equal((await fetch(base + "api/settings", { headers: { "X-MF-Language": "tr", "X-MF-Token": token } })).status, 404, "browser-only settings unexpectedly acquired a server route");
  const after = await (await fetch(base + "api/state", { headers: { "X-MF-Language": "tr", "X-MF-Token": token } })).json();
  assert.equal(after.epoch, before.epoch);
  assert.equal(after.job, before.job);
  console.log("HTTP SETTINGS PASS: local assets only, no settings route and unchanged Octave identity.");
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
