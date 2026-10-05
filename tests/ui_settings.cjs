const { chromium, expect } = require("@playwright/test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ locale: "tr-TR", viewport: { width: 1512, height: 982 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const waitReady = () => page.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır" && typeof document.querySelector("#left-divider")?.onkeydown === "function");
  const storedSettings = () => page.evaluate(() => JSON.parse(localStorage.getItem("mf-settings-v1")));
  const expectLeft = (width, inline = `${width}px`) => expect.poll(() => page.evaluate(() => ({
    inline: document.documentElement.style.getPropertyValue("--left"),
    computed: parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--left")),
    panel: document.querySelector("#left-panel").clientWidth
  }))).toEqual({ inline, computed: width, panel: width });
  const pressFontShortcut = async (key, size, message) => {
    await page.keyboard.press(key);
    assert.equal(await page.evaluate(() => localStorage.getItem('mf-editor-font-size')), String(size), 'font compatibility mirror was not immediate');
    // The font command is synchronous. Observe after dispatch, on the next frame,
    // including negative/once-only checks (not a transient intermediate size).
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => resolve())));
    await expect(page.locator(".cm-editor"), message).toHaveCSS("font-size", `${size}px`);
    await expect.poll(async () => (await storedSettings()).preferences.editorFontSize, { message }).toBe(size);
  };
  const openSettings = async () => {
    await page.locator("#settings").click();
    await expect(page.locator(".settings-dialog")).toBeVisible();
    await expect(page.locator("#modal-title")).toHaveText("Ayarlar");
  };
  const consoleRatio = () => page.evaluate(() => document.querySelector('.console-panel').getBoundingClientRect().width / document.querySelector('.bottom-panels').getBoundingClientRect().width);
  const dragDivider = async (selector, x) => {
    const box = await page.locator(selector).boundingBox();
    assert(box, `missing divider ${selector}`);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(x, box.y + box.height / 2, { steps: 5 });
    await page.mouse.up();
  };
  try {
    await page.goto(launch.url);
    await waitReady();
    assert.equal(await page.evaluate(() => localStorage.getItem('mf-theme')), 'dark');
    await page.locator('#theme').click();
    assert.equal(await page.evaluate(() => localStorage.getItem('mf-theme')), 'light');
    await expect.poll(storedSettings).toMatchObject({ preferences: { theme: 'light' } });
    await page.locator('#theme').click();
    assert.equal(await page.evaluate(() => localStorage.getItem('mf-theme')), 'dark');
    await expect.poll(storedSettings).toMatchObject({ preferences: { theme: 'dark' } });
    await page.evaluate(() => {
      localStorage.removeItem("mf-settings-v1");
      localStorage.setItem("mf-theme", "dark");
      localStorage.setItem("mf-editor-font-size", "13");
    });
    await page.reload();
    await waitReady();
    await openSettings();
    const dialog = page.locator(".settings-dialog");
    await dialog.locator(".settings-field").filter({ hasText: "Tema" }).locator("select").selectOption("light");
    await dialog.locator(".settings-field").filter({ hasText: "Editör yazı boyutu" }).locator("input").fill("15");
    await dialog.locator(".settings-field").filter({ hasText: "Editör yazı boyutu" }).locator("input").press("Tab");
    await dialog.locator(".settings-field").filter({ hasText: "Konsol yazı boyutu" }).locator("input").fill("16");
    await dialog.locator(".settings-field").filter({ hasText: "Konsol yazı boyutu" }).locator("input").press("Tab");
    await dialog.locator(".settings-field").filter({ hasText: "Girinti genişliği" }).locator("input").fill("3");
    await dialog.locator(".settings-field").filter({ hasText: "Girinti genişliği" }).locator("input").press("Tab");
    await dialog.locator(".settings-field").filter({ hasText: "Akıllı girinti" }).locator("select").selectOption("tabs");
    await dialog.locator(".settings-check").filter({ hasText: "Grafikler" }).locator("input").uncheck();
    await page.locator("#modal-close").click();
    await expect(page.locator(".plot-panel")).toBeHidden();
    await expect(page.locator(".cm-editor")).toHaveCSS("font-size", "15px");
    await expect(page.locator("#command")).toHaveCSS("font-size", "16px");
    await expect.poll(storedSettings).toMatchObject({ preferences: { theme: "light", editorFontSize: 15, consoleFontSize: 16, indentWidth: 3, useTabs: true }, panels: { figures: false } });
    await expect.poll(consoleRatio).toBeGreaterThan(0.99);
    await expect.poll(() => page.evaluate(() => [localStorage.getItem('mf-theme'), localStorage.getItem('mf-editor-font-size')])).toEqual(['light', '15']);

    // Exercise the divider's real keyboard handler: default 220px + 10px.
    await expectLeft(220);
    await page.locator("#left-divider").focus();
    await page.keyboard.press("ArrowRight");
    await expectLeft(230);
    await expect.poll(async () => (await storedSettings()).layout.left).toBe(230);
    await page.reload();
    await waitReady();
    await expect(page.locator(".plot-panel")).toBeHidden();
    await expect(page.locator(".cm-editor")).toHaveCSS("font-size", "15px");
    await expect(page.locator("#command")).toHaveCSS("font-size", "16px");
    await expect(page.locator("body")).not.toHaveClass(/\bdark\b/);
    await expectLeft(230);
    await page.setViewportSize({ width: 700, height: 900 });
    // setViewportSize does not wait for our resize handler. Wait for it to
    // remove the inline override AND for the narrow CSS layout to take effect.
    await expectLeft(170, "");
    assert.equal((await storedSettings()).layout.left, 230, "mobile fallback overwrote desktop layout");
    await page.setViewportSize({ width: 1512, height: 982 });
    await expectLeft(230);

    // Finding 4: real same-origin tab receives committed preferences and does
    // not revert another field when it writes its own change.
    const peer = await context.newPage();
    peer.on('pageerror', error => errors.push(error.message));
    await peer.goto(launch.url);
    await expect(peer.locator('.cm-editor')).toHaveCSS('font-size', '15px');
    await openSettings();
    await dialog.locator('.settings-field').filter({ hasText: 'Tema' }).locator('select').selectOption('dark');
    await expect(peer.locator('body')).toHaveClass(/\bdark\b/);
    await peer.locator('#settings').click();
    const peerFont = peer.locator('.settings-field').filter({ hasText: 'Editör yazı boyutu' }).locator('input');
    await peerFont.fill('19');
    await peerFont.press('Tab');
    await expect(page.locator('.cm-editor')).toHaveCSS('font-size', '19px');
    await expect.poll(storedSettings).toMatchObject({ preferences: { theme: 'dark', editorFontSize: 19 } });
    await peer.close();
    await dialog.locator('.settings-field').filter({ hasText: 'Tema' }).locator('select').selectOption('light');
    const fontInput = dialog.locator('.settings-field').filter({ hasText: 'Editör yazı boyutu' }).locator('input');
    await fontInput.fill('15');
    await fontInput.press('Tab');
    await expect.poll(storedSettings).toMatchObject({ preferences: { theme: 'light', editorFontSize: 15 } });
    await page.locator('#modal-close').click();

    // Finding 5: real pointer capture, valid maximum sizes, then widths just
    // above both responsive breakpoints. Desktop storage must stay untouched.
    await dragDivider('#left-divider', 366);
    await expectLeft(360);
    await dragDivider('#right-divider', 1512 - 500 - 6);
    await expect.poll(storedSettings).toMatchObject({ layout: { left: 360, right: 500, consoleWidth: 50 } });
    await page.reload();
    await waitReady();
    await expectLeft(360);
    for (const width of [901, 1151]) {
      await page.setViewportSize({ width, height: 900 });
      await expectLeft(width === 901 ? 190 : 220, '');
      await expect.poll(() => page.evaluate(() => {
        const center = document.querySelector('.center').getBoundingClientRect();
        const right = document.querySelector('#right-panel').getBoundingClientRect();
        return center.width >= 300 && center.right <= innerWidth && right.right <= innerWidth;
      })).toBe(true);
      assert.equal((await storedSettings()).layout.left, 360);
      assert.equal((await storedSettings()).layout.right, 500);
    }
    await page.setViewportSize({ width: 1512, height: 982 });
    await expectLeft(360);
    await page.locator('[data-action="new"]').first().click();
    await page.locator(".cm-content").fill("if true\nx=1;\nend");
    await page.locator(".cm-content").press("ControlOrMeta+a");
    await page.locator(".cm-content").press("Control+i");
    await expect.poll(() => page.locator(".cm-content").innerText()).toBe("if true\n\tx=1;\nend");

    await openSettings();
    const fontRow = page.locator(".shortcut-row").filter({ has: page.locator(".shortcut-name", { hasText: "Editör yazısını büyüt" }) });
    for (const unsafe of ['a', 'Enter', 'Tab', 'ArrowLeft', 'Alt+2']) {
      await fontRow.getByRole('button', { name: 'Editör yazısını büyüt kısayolunu değiştir' }).click();
      await page.keyboard.press(unsafe);
      await expect(fontRow.getByRole('button', { name: 'Editör yazısını büyüt kısayolunu değiştir' })).not.toHaveClass(/capturing/);
      await expect(page.locator('#toast')).toBeVisible();
      assert.equal((await storedSettings()).shortcuts['global.font-increase'], undefined);
    }
    await fontRow.getByRole('button', { name: 'Editör yazısını büyüt kısayolunu değiştir' }).click();
    await page.keyboard.press('ControlOrMeta+Shift+s');
    await expect(page.locator('#toast')).toContainText('Etkin dosyayı kaydet');
    assert.equal((await storedSettings()).shortcuts['global.font-increase'], undefined);
    await fontRow.getByRole("button", { name: "Editör yazısını büyüt kısayolunu değiştir" }).click();
    await page.keyboard.press("ControlOrMeta+u");  // Mod+B now belongs to the Primary Side Bar toggle
    await expect.poll(async () => (await storedSettings()).shortcuts["global.font-increase"]?.map((item) => item.binding)).toEqual(["Mod+KeyU"]);
    await page.locator("#modal-close").click();
    await expect(dialog).toBeHidden();
    await page.locator(".cm-content").focus();
    await pressFontShortcut("ControlOrMeta+=", 15, "old shortcut still ran after rebinding");
    await pressFontShortcut("ControlOrMeta+u", 16, "new shortcut did not run exactly once");

    await openSettings();
    const reboundRow = page.locator(".shortcut-row").filter({ has: page.locator(".shortcut-name", { hasText: "Editör yazısını büyüt" }) });
    await reboundRow.getByRole("button", { name: "Varsayılan" }).click();
    await expect(reboundRow.getByRole("button", { name: "Varsayılan" })).toBeDisabled();
    await expect.poll(async () => (await storedSettings()).shortcuts["global.font-increase"]).toBeUndefined();
    await dialog.getByRole("button", { name: "Yerleşimi sıfırla" }).click();
    await expect.poll(storedSettings).toMatchObject({ layout: { left: 220 }, panels: { figures: true } });
    await page.locator("#modal-close").click();
    await expect(dialog).toBeHidden();
    await expectLeft(220);
    await expect(page.locator(".plot-panel")).toBeVisible();
    await expect.poll(consoleRatio).toBeGreaterThan(0.49);
    await expect.poll(consoleRatio).toBeLessThan(0.51);
    await page.locator(".cm-content").focus();
    await pressFontShortcut("ControlOrMeta+u", 16, "custom binding still ran after restoring defaults");
    await pressFontShortcut("ControlOrMeta+=", 17, "restored shortcut did not run exactly once");

    const malformed = await browser.newContext({ locale: "tr-TR", viewport: { width: 1512, height: 982 } });
    const malformedPage = await malformed.newPage();
    malformedPage.on("pageerror", (error) => errors.push(error.message));
    await malformedPage.goto(launch.url);
    await malformedPage.evaluate(() => localStorage.setItem("mf-settings-v1", "{"));
    await malformedPage.reload();
    await malformedPage.waitForFunction(() => document.querySelector("#status-text")?.textContent === "Hazır");
    await expect(malformedPage.locator("body")).toHaveClass(/\bdark\b/);
    await expect(malformedPage.locator(".cm-editor")).toHaveCSS("font-size", "13px");
    await malformed.close();
    assert.deepEqual(errors, []);
    console.log("UI SETTINGS PASS: preferences, reload, shortcut rebind/restore, layout persistence/reset and malformed storage fallback.");
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
