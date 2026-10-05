const { chromium } = require("@playwright/test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { randomUUID } = require("node:crypto");

const stem = "editor_cmd_" + randomUUID().replaceAll("-", "");
const first = stem + "_first";
const second = stem + "_second";
const third = stem + "_third";
const selected = stem + "_selected";
const hidden = stem + "_hidden";

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ locale: "tr-TR", viewport: { width: 1512, height: 982 } });
  const page = await context.newPage();
  const errors = [];
  let executeCount = 0;
  let fileWriteCount = 0;
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (request.url().endsWith("/api/execute")) executeCount++;
    if (request.url().endsWith("/api/file") && request.method() === "POST") fileWriteCount++;
  });
  const waitIdle = () => page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır");
  const runShortcut = async (shortcut) => {
    const requestsBefore = executeCount;
    const entriesBefore = await page.locator(".console-entry").count();
    await page.keyboard.press(shortcut);
    await page.waitForFunction((count) => document.querySelectorAll(".console-entry").length === count + 1, entriesBefore);
    await waitIdle();
    assert.equal(executeCount, requestsBefore + 1, shortcut + " must send exactly one execute request");
  };
  const assertIgnoredShortcuts = async (target) => {
    const before = { executeCount, fileWriteCount, source: await page.locator(".cm-content").innerText(), title: await page.locator("#modal-title").innerText() };
    for (const key of ["F5", "F9", "ControlOrMeta+s", "Shift+F9"]) await target.press(key);
    await page.waitForTimeout(100);
    assert.equal(executeCount, before.executeCount, "foreign input/modal shortcut executed code");
    assert.equal(fileWriteCount, before.fileWriteCount, "foreign input/modal shortcut saved a file");
    assert.equal(await page.locator(".cm-content").innerText(), before.source);
    assert.equal(await page.locator("#modal-title").innerText(), before.title, "shortcut replaced the active dialog");
  };
  try {
    await page.goto(launch.url);
    await waitIdle();
    await page.locator('[data-action="new"]').first().click();
    const menu = page.locator(".editor-menu");
    assert.equal(await menu.count(), 1, "editor menu must be unique");
    assert.equal(await page.locator(".run-more summary").count(), 1, "existing Diğer selector must remain unique");
    assert.equal(await page.locator(".run-more summary").innerText(), "Diğer");
    assert.equal(await page.locator(".run-more.editor-menu").count(), 0, "menus must have independent selectors");
    await menu.locator("summary").click();
    assert.deepEqual(await menu.locator('[role="menuitem"]').allTextContents(), ["Bölümü çalıştır⌘↵", "Çalıştır ve ilerle⇧⌘↵", "Bölümden sona çalıştır⌥⇧⌘↵", "Seçimi çalıştırF9", "Satıra git⌃G", "Yorumu aç / kapat⌘/ · ⇧F9", "Akıllı girinti⌃I", "Tümünü daralt⌃,", "Tümünü genişlet⌃."]);
    await menu.locator("summary").click();

    const sectionFixture = `%% Bir\n${first} = 101;\n%% İki\n${second} = 202;\n%% Üç\n${third} = 303;\n${selected} = 404;`;
    await page.locator(".cm-content").fill(sectionFixture);
    const historyBefore = await page.evaluate(async () => await (await fetch("/api/history", { headers: { "X-MF-Token": sessionStorage.getItem("mf-token") } })).json());

    await page.locator(".cm-line").nth(1).click();
    await page.keyboard.press("Home");
    await runShortcut("ControlOrMeta+Enter");
    assert((await page.locator("#variables").innerText()).includes(first));
    assert(!(await page.locator("#variables").innerText()).includes(second));
    assert.equal(await page.locator(".console-command").last().innerText(), `>> Bölüm · yeni_1.m:1–2`);
    assert((await page.locator(".console-command").last().getAttribute("title")).includes(first));

    await page.locator(".cm-line").nth(1).click();
    await page.keyboard.press("Home");
    await runShortcut("Shift+ControlOrMeta+Enter");
    assert((await page.locator("#cursor").innerText()).startsWith("Satır 3,"), "run and advance did not select the next section start");

    await page.locator(".cm-line").nth(5).click();
    await page.keyboard.press("Home");
    const lastSectionCursor = await page.locator("#cursor").innerText();
    await runShortcut("Shift+ControlOrMeta+Enter");
    assert.equal(await page.locator("#cursor").innerText(), lastSectionCursor, "last section moved the cursor");

    await page.locator(".cm-line").nth(2).click();
    await page.keyboard.press("Home");
    await runShortcut("Shift+Alt+ControlOrMeta+Enter");
    const variablesAfterEnd = await page.locator("#variables").innerText();
    assert(variablesAfterEnd.includes(second));
    assert(variablesAfterEnd.includes(third));
    assert.equal(await page.locator(".console-command").last().innerText(), `>> Sona kadar · yeni_1.m:3–7`);

    await page.locator(".cm-line").nth(6).click();
    await page.keyboard.press("Home");
    await page.keyboard.down("Shift");
    await page.keyboard.press("End");
    await page.keyboard.up("Shift");
    await runShortcut("F9");
    assert((await page.locator("#variables").innerText()).includes(selected));
    assert.equal(await page.locator(".console-command").last().innerText(), ">> Seçim · 1 satır");
    const historyAfter = await page.evaluate(async () => await (await fetch("/api/history", { headers: { "X-MF-Token": sessionStorage.getItem("mf-token") } })).json());
    assert.deepEqual(historyAfter, historyBefore, "editor run commands leaked wrapper/source text into command history");

    // Finding 11: preserve command-line F9, but ignore Shift-F9 there.
    await page.locator("#command").focus();
    await runShortcut("F9");
    const commandRequests = executeCount;
    await page.locator("#command").press("Shift+F9");
    await page.waitForTimeout(100);
    assert.equal(executeCount, commandRequests);

    // Finding 6: all three run variants, decoration and folding share the
    // same lexical boundaries. The fake header must never expose hidden code.
    const lexicalFixture = `%% active\n%{\n%% fake\n${hidden} = 999;\n%}\n${first} = 111;\n%% next\n${second} = 222;`;
    await page.locator(".cm-content").fill(lexicalFixture);
    assert.deepEqual(await page.locator(".cm-section").allTextContents(), ["%% active", "%% next"]);
    await page.locator(".cm-line").nth(3).click();
    await runShortcut("ControlOrMeta+Enter");
    assert(!(await page.locator("#variables").innerText()).includes(hidden));
    assert.equal(await page.locator(".console-command").last().getAttribute("title"), lexicalFixture.split("\n").slice(0, 6).join("\n"));
    await runShortcut("Shift+ControlOrMeta+Enter");
    assert((await page.locator("#cursor").innerText()).startsWith("Satır 7, Sütun 1"));
    await page.locator(".cm-line").nth(3).click();
    await runShortcut("Shift+Alt+ControlOrMeta+Enter");
    assert.equal(await page.locator(".console-command").last().getAttribute("title"), lexicalFixture);
    assert(!(await page.locator("#variables").innerText()).includes(hidden));
    await page.keyboard.press("Control+,");
    assert.equal(await page.locator(".cm-foldPlaceholder").count(), 2);
    assert(!(await page.locator(".cm-content").innerText()).includes("%% fake"));
    await page.keyboard.press("Control+.");
    assert.equal(await page.locator(".cm-foldPlaceholder").count(), 0);

    // Finding 12: comment/uncomment a section-only selection without losing %%.
    await page.locator(".cm-content").fill("%% Section");
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.press("Shift+F9");
    assert.equal(await page.locator(".cm-content").innerText(), "% %% Section");
    await page.keyboard.press("Shift+F9");
    assert.equal(await page.locator(".cm-content").innerText(), "%% Section");

    const commentFixture = "  alpha = 1;\n    beta = 2;";
    await page.locator(".cm-content").fill(commentFixture);
    await page.locator(".cm-content").click();
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.press("ControlOrMeta+/");
    assert.equal(await page.locator(".cm-content").innerText(), "  % alpha = 1;\n    % beta = 2;");
    await page.keyboard.press("ControlOrMeta+/");
    assert.equal(await page.locator(".cm-content").innerText(), commentFixture);
    const beforeCommentShortcut = executeCount;
    await page.keyboard.press("Shift+F9");
    assert.equal(await page.locator(".cm-content").innerText(), "  % alpha = 1;\n    % beta = 2;");
    await page.keyboard.press("Shift+F9");
    assert.equal(await page.locator(".cm-content").innerText(), commentFixture);
    assert.equal(executeCount, beforeCommentShortcut, "Shift+F9 must not fall through to Run Selection");

    const indentFixture = "function y = f(x)\nif x\nswitch x\ncase 1\ny = 'end % stays';\notherwise\n% if ignored\ny = 0;\nendswitch\nelse\ntry\ny = 2;\ncatch\ny = 3;\nend_try_catch\nendif\nendfunction";
    const indentedFixture = "function y = f(x)\n    if x\n        switch x\n            case 1\n                y = 'end % stays';\n            otherwise\n                % if ignored\n                y = 0;\n        endswitch\n    else\n        try\n            y = 2;\n        catch\n            y = 3;\n        end_try_catch\n    endif\nendfunction";
    await page.locator(".cm-content").fill(indentFixture);
    await page.keyboard.press("ControlOrMeta+a");
    await page.keyboard.press("Control+i");
    assert.equal(await page.locator(".cm-content").innerText(), indentedFixture);

    await page.locator(".run-more summary").click();
    await menu.locator("summary").click();
    await page.locator("#editor-go-line").click();
    assert.equal(await menu.getAttribute("open"), null, "editor action must close its own menu");
    assert.notEqual(await page.locator(".run-more").getAttribute("open"), null, "editor action must not close Diğer");
    await assertIgnoredShortcuts(page.getByLabel("Satır numarası"));
    await page.getByLabel("Satır numarası").fill("12");
    await page.locator(".go-line-dialog button").click();
    assert((await page.locator("#cursor").innerText()).startsWith("Satır 12, Sütun 1"));
    await page.locator(".run-more summary").click();

    await page.locator('[data-action="save-as"]').click();
    await assertIgnoredShortcuts(page.getByLabel("Dosya adı"));
    await page.locator("#modal-close").click();

    // A second native dialog models other features' path dialogs without
    // depending on their lane modules. Also cover editable controls outside it.
    await page.evaluate(() => {
      const dialog = document.createElement("dialog");
      dialog.id = "editor-review-dialog";
      const input = document.createElement("input");
      input.id = "editor-review-path";
      input.setAttribute("aria-label", "Regression MAT path");
      dialog.append(input);
      document.body.append(dialog);
      dialog.showModal();
    });
    await assertIgnoredShortcuts(page.locator("#editor-review-path"));
    await page.evaluate(() => document.querySelector("#editor-review-dialog").remove());
    for (const tag of ["input", "textarea", "select", "div"]) {
      await page.evaluate((tag) => {
        const control = document.createElement(tag);
        control.id = "editor-review-control";
        if (tag === "div") control.contentEditable = "true";
        document.body.append(control);
      }, tag);
      await assertIgnoredShortcuts(page.locator("#editor-review-control"));
      await page.evaluate(() => document.querySelector("#editor-review-control").remove());
    }

    await page.locator(".cm-content").fill("function y = fold_fixture(x)\nif x\ny = 1;\nend\nelsewhere = 2;\nend");
    await page.locator(".cm-content").click();
    await page.keyboard.press("Control+,");
    await page.waitForFunction(() => document.querySelectorAll(".cm-foldPlaceholder").length >= 1);
    await page.keyboard.press("Control+.");
    await page.waitForFunction(() => document.querySelectorAll(".cm-foldPlaceholder").length === 0);

    await page.locator("#command").fill("while true; end");
    await page.locator("#command").press("Enter");
    await page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hesaplanıyor");
    for (const selector of ["#editor-run-section", "#editor-run-advance", "#editor-run-to-end", "#editor-run-selection"]) assert(await page.locator(selector).isDisabled(), selector + " was enabled while busy");
    const busyRequests = executeCount;
    await page.locator(".cm-content").click();
    await page.keyboard.press("ControlOrMeta+Enter");
    await page.waitForFunction(() => document.querySelector("#toast")?.textContent === "Önce çalışan işlemi veya hata ayıklamayı bitir.");
    assert.equal(executeCount, busyRequests, "busy shortcut sent an execute request");
    await page.locator("#stop").click();
    await waitIdle();

    const cleanupEntries = await page.locator(".console-entry").count();
    await page.locator("#command").fill(`clear ${first} ${second} ${third} ${selected} ${hidden};`);
    await page.locator("#command").press("Enter");
    await page.waitForFunction((count) => document.querySelectorAll(".console-entry").length === count + 1, cleanupEntries);
    await waitIdle();
    assert.deepEqual(errors, []);
    console.log("UI EDITOR COMMANDS PASS: menu, run shortcuts, section boundaries/advance, history, comment, indent, go-to-line, fold, busy refusal.");
  } finally {
    await context.close();
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
