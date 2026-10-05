const { chromium, expect } = require("@playwright/test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { randomUUID } = require("node:crypto");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const state = await (await fetch(base + "api/state", { headers: { "X-MF-Token": token } })).json();
  const name = "intel_" + randomUUID().replaceAll("-", "");
  const file = path.join(state.current, name + ".m");
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ locale: "tr-TR", viewport: { width: 1512, height: 982 } });
  const errors = [];
  let executeCount = 0;
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (request.url().endsWith("/api/execute")) executeCount++;
  });
  const content = page.locator("#editor .cm-content");
  const panel = page.locator('#editor-intel-panel[data-view="occurrences"]');
  const waitUses = async (symbol, scope, count) => {
    await expect(panel).toBeVisible();
    await expect(panel.locator("#editor-intel-title")).toHaveText(`“${symbol}” kullanımları`);
    await expect(panel.locator(".editor-intel-label")).toHaveCount(1);
    await expect(panel.locator(".editor-intel-label")).toHaveText(scope);
    await expect(panel.locator(".editor-intel-row")).toHaveCount(count);
  };
  const goLine = async (number) => {
    await content.focus();
    await page.keyboard.press("Control+g");
    await page.getByLabel("Satır numarası", { exact: true }).fill(String(number));
    await page.locator(".go-line-dialog button").click();
    await expect(page.locator("#cursor")).toHaveText(`Satır ${number}, Sütun 1`);
    await expect(content).toBeFocused();
  };
  try {
    const indexed = page.waitForResponse((response) => response.url().endsWith("/api/symbols") && response.status() === 200);
    await page.goto(launch.url);
    await page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır");
    await indexed;
    fs.writeFileSync(file, `function y = ${name}(x)\ny = x + 1;\nend\n`);
    const reindexed = page.waitForResponse((response) => response.url().endsWith("/api/symbols") && response.status() === 200);
    await page.locator("#refresh-files").click();
    await reindexed;
    await page.locator('[data-action="new"]').first().click();
    const source = `%% Birinci\nvalue = 1;\ntext = 'function fake(x)';\n%{\n%% sahte\nfunction hidden(x)\n%}\n%% İkinci\nfunction y = local_fn(x)\ny = x + value;\nend`;
    await page.locator(".cm-content").fill(source);
    await page.locator("#toggle-outline").click();
    await expect(page.locator('#editor-intel-panel[data-view="outline"] .editor-intel-label')).toHaveText("Etkin dosya anahattı");
    await expect(page.locator("#editor-intel-content .editor-intel-name")).toHaveText(["Birinci", "İkinci", "local_fn"]);
    assert.equal(executeCount, 0, "live outline touched the persistent Octave session");
    await page.locator(".cm-line").nth(9).click();
    await page.waitForFunction(() => document.querySelector("#editor-intel-content .editor-intel-row.current .editor-intel-name")?.textContent === "local_fn");
    await page.locator("#editor-intel-content .editor-intel-row").first().click();
    await expect(page.locator("#cursor")).toHaveText("Satır 1, Sütun 1");

    await page.locator(".cm-content").fill("localValue = 1;\nresult = localValue + 2;");
    await goLine(2);
    for (let index = 0; index < 9; index++) await page.keyboard.press("ArrowRight");
    await expect(page.locator("#cursor")).toHaveText("Satır 2, Sütun 10");
    await content.press("Alt+F8");
    await waitUses("localValue", "Yalnız etkin dosya", 2);
    // Cursor and asynchronous lint updates must not erase the usages view.
    await content.press("ArrowRight");
    await page.waitForFunction(() => document.querySelector("#code-issues")?.textContent === "Sorun yok");
    await waitUses("localValue", "Yalnız etkin dosya", 2);
    await content.press("Alt+F7");
    await expect(page.locator("#cursor")).toHaveText("Satır 1, Sütun 1");

    await page.locator(".cm-content").fill(name.slice(0, -2));
    await page.locator(".cm-content").press("Control+Space");
    await expect(page.locator("#editor .cm-tooltip-autocomplete")).toContainText(name);
    await page.keyboard.press("Escape");

    await page.locator(".cm-content").fill(`${name}(3);`);
    await goLine(1);
    await content.press("Alt+F8");
    await waitUses(name, "Etkin dosya ve geçerli klasör", 2);
    await content.press("Alt+F7");
    await expect(page.locator("#editor-label")).toHaveText(name + ".m");
    await expect(page.locator("#cursor")).toHaveText("Satır 1, Sütun 1");

    await page.locator('[data-action="new"]').first().click();
    await page.locator(".cm-content").fill("x = ;");
    await page.waitForFunction(() => document.querySelector("#code-issues")?.textContent === "1 sözdizimi hatası, satır 1");
    await page.locator("#toggle-code-issues").click();
    await expect(page.locator('#editor-intel-panel[data-view="issues"] .editor-intel-label')).toHaveText("Octave sözdizimi");
    await expect(page.locator("#editor-intel-content .editor-intel-row")).toHaveCount(1);
    await page.locator('[data-action="new"]').first().click();
    await page.locator(".cm-content").fill("x = 1;");
    await page.waitForFunction(() => document.querySelector("#code-issues")?.textContent === "Sorun yok");
    await expect(page.locator("#editor-intel-content .muted-empty")).toHaveText("Octave sözdizimi sorunu yok.");
    await expect(page.locator("#editor-intel-content .editor-intel-row")).toHaveCount(0);
    assert.deepEqual(errors, []);
    console.log("UI EDITOR INTEL PASS: live outline, folder completion, Alt-F7/F8 navigation, persistent occurrences and active-tab Octave issues.");
  } finally {
    await browser.close();
    fs.rmSync(file, { force: true });
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
