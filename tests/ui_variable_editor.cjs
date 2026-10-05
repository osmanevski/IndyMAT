const assert = require("assert");
const fs = require("fs");
const { randomUUID } = require("crypto");
const { chromium } = require("playwright");

(async () => {
  const launch = JSON.parse(fs.readFileSync(".matlab-free/launch.json", "utf8"));
  const [base, token] = launch.url.split("#");
  const request = (endpoint, body) => fetch(base + endpoint, { method: body === undefined ? "GET" : "POST", headers: { "X-MF-Token": token, ...body === undefined ? {} : { "Content-Type": "application/json" } }, body: body === undefined ? undefined : JSON.stringify(body) });
  const waitIdle = async () => {
    for (let index = 0; index < 400; index++) {
      let state = await (await request("api/state")).json();
      if (state.status === "idle") return state;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error("UI variable editor setup timeout");
  };
  const suffix = randomUUID().replaceAll("-", "");
  const matrix = "ui_ve_" + suffix + "_matrix", nd = "ui_ve_" + suffix + "_nd", cell = "ui_ve_" + suffix + "_cell", text = 'ui_ve_' + suffix + '_text', chars = 'ui_ve_' + suffix + '_chars';
  const scalar = 'ui_ve_' + suffix + '_scalar';
  let browser;
  try {
    let response = await request("api/execute", { mode: "code", code: `${matrix}=int16(reshape(1:3675,[105,35]));${nd}=reshape(1:24,[2,3,4]);${cell}={struct('field',7),[8 9]};${text}='ğ';${chars}=char('ğ','ab');${scalar}=1;` });
    assert.equal(response.status, 202);
    await waitIdle();
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({ locale: "tr-TR", permissions: ["clipboard-read", "clipboard-write"] });
    const page = await context.newPage();
    await page.goto(launch.url);
    await page.waitForFunction((name) => document.querySelector(`#variables tr[data-name="${name}"]`), matrix);
    // Keep the existing ui_workspace close/reopen sequence in this feature's gate.
    const scalarRow = page.locator(`#variables tr[data-name="${scalar}"]`);
    await scalarRow.click();
    assert.equal(await page.locator('#modal[open]').count(),0);
    for (const column of [0,1,2,3]) {
      await scalarRow.locator('td').nth(column).dblclick();
      await page.locator('#modal[open]').waitFor();
      assert.equal(await page.locator('#modal-title').innerText(),`${scalar} — Değişken görünümü`);
      assert.equal(await page.locator('.workspace-inline').count(),0);
      await page.locator('#modal-close').click();
    }
    await scalarRow.focus();
    await scalarRow.press('Enter');
    await page.locator('#modal[open]').waitFor();
    assert.equal(await page.locator('#modal-title').innerText(),`${scalar} — Değişken görünümü`);
    assert.equal(await page.locator('.workspace-inline').count(),0);
    await page.locator('#modal-close').click();
    await page.locator(`#variables tr[data-name="${matrix}"]`).dblclick();
    await page.locator("#modal[open] .variable-grid").waitFor();
    assert.equal(await page.locator("#modal-title").innerText(), `${matrix} — Değişken görünümü`);
    assert((await page.locator(".variable-limit").innerText()).includes("100 satır × 30 sütun"));
    assert((await page.locator(".variable-limit").innerText()).includes("sayfalara bölündü"));
    await page.getByRole("button", { name: "Satır ↓", exact: true }).click();
    await page.waitForFunction(() => document.querySelector(".variable-position")?.textContent.startsWith("101–105"));
    await page.getByRole("button", { name: "↑ Satır", exact: true }).click();
    await page.waitForFunction(() => document.querySelector(".variable-position")?.textContent.startsWith("1–100"));
    let first = page.locator(".variable-grid td[data-row='0'][data-column='0']");
    await first.dblclick();
    await page.locator('.variable-cell-input').fill('x');
    await page.locator('.variable-cell-input').press('Enter');
    await page.waitForFunction(() => document.querySelector('#toast')?.textContent.includes('Tam sayı'));
    assert.equal(await page.locator('.variable-cell-input').count(), 1, 'invalid edit stays correctable');
    await page.locator(".variable-cell-input").fill("123");
    await page.locator(".variable-cell-input").press("Enter");
    await page.waitForFunction(() => document.querySelector(".variable-grid td[data-row='0'][data-column='0']")?.textContent === "123");
    await first.dblclick();
    await page.locator('.variable-cell-input').fill('x');
    await page.locator('.variable-cell-input').press('Enter');
    await page.locator('.variable-cell-input').press('Escape');
    await page.waitForFunction(() => !document.querySelector('.variable-cell-input'));
    assert.equal(await page.locator('#modal[open]').count(), 1, 'Escape cancels only the inline edit');
    await first.click();
    await page.locator(".variable-grid td[data-row='1'][data-column='1']").click({ modifiers: ["Shift"] });
    await page.getByRole("button", { name: "Seçimi kopyala", exact: true }).click();
    let copied = await page.evaluate(() => navigator.clipboard.readText());
    assert(copied.includes("\t") && copied.includes("\n"));
    await first.click();
    await page.evaluate(() => navigator.clipboard.writeText("10\t11\n12\t13"));
    await page.getByRole("button", { name: "Panodan yapıştır", exact: true }).click();
    await page.waitForFunction(() => document.querySelector(".variable-grid td[data-row='1'][data-column='1']")?.textContent === "13");
    let writesAfterPaste = 0;
    const observeWrite = request => {
      if (request.url().endsWith('/api/variable') && request.method() === 'POST' && request.postDataJSON()?.action === 'write') writesAfterPaste++;
    };
    page.on('request', observeWrite);
    await first.dblclick();
    await page.locator('.variable-cell-input').fill('44');
    await page.locator("#modal-close").click();
    await page.waitForFunction(() => !document.querySelector('#modal').open);
    // Two normal poll cycles: neither a blur job nor a completion may reopen it.
    await page.waitForTimeout(1000);
    assert.equal(writesAfterPaste, 0, 'close must not submit a dirty inline draft');
    assert.equal(await page.locator('#modal[open]').count(), 0);
    page.off('request', observeWrite);
    await page.locator(`#variables tr[data-name="${nd}"]`).dblclick();
    await page.locator("input[aria-label='3. boyut dilimi']").waitFor();
    await page.locator("input[aria-label='3. boyut dilimi']").fill("4");
    await page.locator("input[aria-label='3. boyut dilimi']").press("Enter");
    await page.waitForFunction(() => document.querySelector("input[aria-label='3. boyut dilimi']")?.value === "4" && document.querySelector(".variable-grid td[data-row='0'][data-column='0']")?.textContent === "19");
    await page.locator("#modal-close").click();
    await page.locator(`#variables tr[data-name="${cell}"]`).dblclick();
    await page.locator(".variable-grid td[data-row='0'][data-column='0']").dblclick();
    await page.waitForFunction(() => document.querySelector(".variable-breadcrumb")?.textContent.includes("{1,1}") && document.querySelector(".variable-fields"));
    assert((await page.locator(".variable-limit").innerText()).includes("salt okunurdur"));
    await page.locator('#modal-close').click();
    await page.locator(`#variables tr[data-name="${text}"]`).dblclick();
    await page.locator('.variable-text-input').waitFor();
    assert.equal(await page.locator('.variable-text-input').inputValue(), 'ğ');
    assert((await page.locator('.variable-limit').innerText()).includes('bayt uzunluğunu değiştirebilir'));
    await page.locator('.variable-text-input').fill('İstanbul 🙂');
    await page.getByRole('button', {name:'Metni kaydet',exact:true}).click();
    await page.waitForFunction(() => document.querySelector('.variable-meta')?.textContent.includes('1 × 14') && document.querySelector('.variable-text-input')?.value === 'İstanbul 🙂');
    await page.locator('#modal-close').click();
    await page.locator(`#variables tr[data-name="${chars}"]`).dblclick();
    await page.waitForFunction(() => document.querySelector('.variable-detail')?.textContent === 'ğ\nab');
    assert.equal(await page.locator('.variable-text-input').count(), 0);
    assert((await page.locator('.variable-limit').innerText()).includes('eşit değil'));
    assert(!(await page.locator('#modal-body').innerText()).includes('�'));
    console.log("UI VARIABLE EDITOR PASS: repeated workspace close/reopen and Enter, paging, cell edit, rectangular copy/paste, N-D slices, nested read-only navigation and limit notices.");
  } finally {
    if (browser) await browser.close();
    try {
      let state = await (await request("api/state")).json();
      if (state.status === "idle") {
        let names = [matrix, nd, cell, text, chars, scalar].filter((name) => state.variables.some((item) => item.name === name));
        if (names.length) {
          await request("api/workspace", { action: "clear-names", names, confirm: true });
          await waitIdle();
        }
      }
    } catch {}
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
