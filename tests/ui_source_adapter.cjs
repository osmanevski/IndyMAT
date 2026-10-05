const { chromium, expect } = require("@playwright/test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ locale: "tr-TR", viewport: { width: 1512, height: 982 } });
  const page = await context.newPage();
  const errors = [], requests = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => { if (request.url().endsWith("/api/execute")) requests.push(request.postDataJSON()); });
  const idle = () => page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır");
  const editor = page.locator(".cm-content");
  const submitAndWait = async (action) => {
    const before = requests.length;
    const response = page.waitForResponse((reply) => reply.url().endsWith("/api/execute") && reply.request().method() === "POST");
    await action();
    const reply = await response;
    assert.equal(reply.status(), 202);
    const accepted = await reply.json();
    await expect.poll(() => requests.length).toBe(before + 1);
    // A request event can precede acceptance while the previous job still
    // shows Ready. Wait for this job's completed console entry first.
    await expect(page.locator(`.console-entry[data-job="${accepted.job}"] .console-time`)).toHaveText(/\d+(?:\.\d+)? s/);
    await idle();
  };
  const selection = async (code) => {
    await editor.fill(code);
    await editor.press("ControlOrMeta+a");
    await submitAndWait(() => editor.press("F9"));
  };
  try {
    await page.goto(launch.url);
    await idle();
    await page.locator('[data-action="new"]').first().click();
    await selection('r="ab"+"cd";');
    assert.deepEqual(requests.at(-1), { code: 'r="ab"+"cd";', mode: "code", argument: "", history: false });
    await expect(page.locator(".source-adapter-note")).toHaveCount(0);
    await page.locator("#settings").click();
    const setting = page.getByRole("checkbox", { name: "Editördeki çift tırnaklı metin sabitlerini uyarla" });
    await expect(setting).not.toBeChecked();
    await expect(page.locator(".settings-note")).toContainText("Deneysel:");
    await expect(page.locator(".settings-note")).toContainText("Kayıtlı dosyayı çalıştırma, Konsol, Yayınla, asistan kodu");
    await setting.check();
    await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("mf-settings-v1")).preferences.adaptEditorLiterals)).toBe(true);
    await page.locator("#modal-close").click();
    await page.reload();
    await idle();
    await page.locator("#settings").click();
    await expect(setting).toBeChecked();
    await page.locator("#modal-close").click();
    await selection('r = "ab" + "cd"; total=sum([1 2;3 4],"all");');
    assert.equal(requests.at(-1).adapt_editor_literals, true);
    assert.equal(requests.at(-1).source_context.document, requests.at(-1).code);
    await expect(page.locator(".source-adapter-note").last()).toHaveText("Uyarlandı: 3 metin sabiti");
    await expect(page.locator(".source-adapter-note").last()).toHaveAttribute("title", /Özgün satırlar: 1/);
    await expect(page.locator(".console-source").last()).toHaveText(requests.at(-1).code);
    await expect(page.locator("#variables")).toContainText("string");
    await selection('r="x"; disp "hello"');
    await expect(page.locator(".source-adapter-note").last()).toHaveText("Değiştirilmeden çalıştırıldı: Tırnaklı komut biçimi argümanları değiştirilmeden çalıştırılır.");
    const failing = 'attempts=0;\nattempts=attempts+1; r="x"; missing_adapter_ui;';
    await selection(failing);
    const column = failing.split("\n")[1].indexOf("missing_adapter_ui") + 1;
    const link = page.locator(".source-error-location").last();
    await expect(link).toHaveText(`Özgün kaynak: satır 2, sütun ${column}`);
    await link.click();
    await expect(page.locator("#cursor")).toContainText(`Satır 2, Sütun ${column}`);
    await editor.fill('r="changed";');
    await link.click();
    await expect(page.locator("#modal-title")).toHaveText("Gönderilen editör görüntüsü");
    await expect(page.locator(".source-snapshot")).toHaveValue(failing);
    await page.locator("#modal-close").click();
    // Section, advance and to-end share the explicit editor text contract.
    for (const [shortcut, origin] of [["ControlOrMeta+Enter","editor-section"], ["Shift+ControlOrMeta+Enter","editor-advance"], ["Shift+Alt+ControlOrMeta+Enter","editor-to-end"]]) {
      await editor.fill('%% first\nr="a";\n%% next\ns="b";');
      await page.locator(".cm-line").nth(1).click();
      await submitAndWait(() => page.keyboard.press(shortcut));
      assert.equal(requests.at(-1).source_context.origin, origin);
      if (origin === "editor-advance") await expect(page.locator("#cursor")).toContainText("Satır 3, Sütun 1");
    }
    await editor.fill('r="x"; n=input(\'Number: \');');
    await editor.press("ControlOrMeta+a");
    await editor.press("F9");
    await expect(page.locator("#input-bar")).toBeVisible();
    await page.locator("#runtime-input").fill("17");
    await page.locator("#runtime-input").press("Enter");
    await idle();
    await editor.fill('r="x"; while true; end');
    await editor.press("ControlOrMeta+a");
    await editor.press("F9");
    await expect(page.locator("#stop")).toBeEnabled();
    await page.locator("#stop").click();
    await idle();
    await page.locator("#command").fill('command_native="x"; assert(attempts==1);');
    await submitAndWait(() => page.locator("#command").press("Enter"));
    assert.equal(requests.at(-1).adapt_editor_literals, undefined);
    await expect(page.locator(".console-error").last()).toHaveText("");
    await page.locator("#settings").click();
    await setting.uncheck();
    await page.locator("#modal-close").click();
    await selection('r="native_again";');
    assert.equal(requests.at(-1).source_context, undefined);
    assert.deepEqual(errors, []);
    console.log("UI SOURCE ADAPTER PASS: 12 scenarios; Turkish settings/default/persistence, native payload, selection/section/advance/to-end, notes, snapshot-bound original columns, input and Stop.");
  } finally {
    await context.close();
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
