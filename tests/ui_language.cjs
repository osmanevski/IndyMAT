const fs = require("node:fs");
const assert = require("node:assert/strict");
const { chromium, expect } = require("@playwright/test");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const contexts = [];
  const open = async (locale) => {
    const context = await browser.newContext({ locale, viewport: { width: 1512, height: 982 } });
    contexts.push(context);
    const page = await context.newPage();
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(launch.url);
    await expect(page.locator("body")).toHaveCSS("visibility", "visible");
    await expect(page.locator(".cm-editor")).toBeVisible();
    return page;
  };
  const selectLanguage = async (page, language) => {
    await page.locator("#settings").click();
    const control = page.locator('.settings-field').filter({ has: page.locator('[data-i18n="Language"]') }).locator("select");
    await expect(control).toBeVisible();
    assert.deepEqual(await control.locator("option").allTextContents(), ["System", "English", "Türkçe"]);
    await control.selectOption(language);
    await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("mf-settings-v1"))?.preferences.language)).toBe(language);
    await page.locator("#modal-close").click();
  };
  try {
    const tr = await open("tr-TR");
    await expect(tr.locator("html")).toHaveAttribute("lang", "tr");
    await expect(tr.locator("#run")).toHaveText("Çalıştır");
    await expect(tr.locator("#run-section")).toHaveAttribute("title", / — Run Section$/);
    await expect(tr.locator("#run")).toHaveAttribute("title", / — Run$/);
    await expect(tr.locator("#left-panel .panel-heading h2")).toHaveText("Geçerli klasör");

    const en = await open("en-US");
    await expect(en.locator("html")).toHaveAttribute("lang", "en");
    await expect(en.locator("#run")).toHaveText("Run");
    await expect(en.locator("#right-panel h2:has(#variable-count)")).toContainText("Workspace");
    await expect(en.locator("#run-section")).toHaveText("Run Section");
    await expect(en.locator("#run-section")).toHaveAttribute("title", "Run the %% section at the cursor (⌘Enter)");
    // A page-global sentinel disappears on reload; switching must preserve it.
    await en.evaluate(() => { window.languageTestSentinel = "still-here"; });
    await selectLanguage(en, "tr");
    await expect(en.locator("#run")).toHaveText("Çalıştır");
    await expect(en.locator("#settings")).toHaveText("Ayarlar");
    await expect(en.locator("#run-section")).toHaveAttribute("title", / — Run Section$/);
    assert.equal(await en.evaluate(() => window.languageTestSentinel), "still-here");
    await en.reload();
    await expect(en.locator("#run")).toHaveText("Çalıştır");
    await expect(en.locator("html")).toHaveAttribute("lang", "tr");
    await selectLanguage(en, "en");
    await expect(en.locator("#run")).toHaveText("Run");
    await expect(en.locator("#settings")).toHaveText("Settings");
    await en.reload();
    await expect(en.locator("#run")).toHaveText("Run");
    await expect(en.locator("html")).toHaveAttribute("lang", "en");
    await selectLanguage(en, "system");
    await expect(en.locator("#run")).toHaveText("Run");
    await selectLanguage(tr, "en");
    await expect(tr.locator("#run")).toHaveText("Run");
    await selectLanguage(tr, "system");
    await expect(tr.locator("#run")).toHaveText("Çalıştır");
    assert.deepEqual(errors, []);
    console.log("UI LANGUAGE PASS: system locales, live switch, override persistence and Turkish MATLAB-term titles.");
  } finally {
    for (const context of contexts) await context.close();
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
