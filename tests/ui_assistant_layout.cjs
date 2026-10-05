const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const { assistantServer } = require('./assistant_harness.cjs');

(async () => {
  const server = await assistantServer();
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({ locale: 'tr-TR', viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    await page.goto(server.launch.url);
    await page.locator('.cm-line').first().waitFor();
    const widths = () => page.evaluate(() => ({
      page: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      right: document.querySelector('#right-panel').getBoundingClientRect().width,
      left: document.querySelector('#left-panel').getBoundingClientRect().width,
      center: document.querySelector('.center').getBoundingClientRect().width,
      heading: document.querySelector('.console-panel .panel-heading h2').getBoundingClientRect().height
    }));
    const before = await widths();
    await page.locator('#toggle-assistant').click();
    await page.locator('#assistant-input').waitFor();
    await page.waitForFunction(() => [...document.querySelector('#assistant-provider').options].every((option) => !option.disabled));
    // A fourth column at 1440 px: no horizontal scroll, the side panels give way, headings stay on one line.
    const open = await widths();
    assert.equal(open.page, 0);
    assert(open.right <= 250 && open.left <= 190, JSON.stringify(open));
    assert(open.center >= 500, JSON.stringify(open));
    assert(open.heading <= before.heading + 1, 'console heading must not wrap');
    // Dragging the Workspace divider while the assistant is open sizes the Workspace panel, not the gap to the window edge.
    const edge = await page.evaluate(() => document.querySelector('#right-panel').getBoundingClientRect().right);
    const divider = await page.locator('#right-divider').boundingBox();
    await page.mouse.move(divider.x + 2, divider.y + 200);
    await page.mouse.down();
    await page.mouse.move(edge - 230, divider.y + 200, { steps: 4 });
    await page.mouse.up();
    const dragged = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--right')));
    assert(Math.abs(dragged - 230) <= 2, 'right width after drag: ' + dragged);
    // Provider tabs drive the provider; the composer is one box with the controls in its footer.
    await page.locator('#assistant-tab-codex').click();
    assert.equal(await page.locator('#assistant-provider').inputValue(), 'codex');
    assert.equal(await page.locator('#assistant-tab-codex').getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('#assistant-tab-claude').getAttribute('aria-selected'), 'false');
    const overlap = await page.evaluate(() => {
      const send = document.querySelector('#assistant-send').getBoundingClientRect();
      return [...document.querySelectorAll('.assistant-selectors select')].some((select) => { const box = select.getBoundingClientRect(); return box.width > 2 && box.right > send.left + 1 && box.left < send.right; });
    });
    assert.equal(overlap, false, 'footer selectors must not cover the send button');
    // The model and effort chips join the same compact footer at its minimum panel width.
    await page.evaluate(() => { document.querySelector('#assistant-panel').style.width = '300px'; });
    const compact = await page.evaluate(() => {
      const send = document.querySelector('#assistant-send').getBoundingClientRect();
      return ['#assistant-model', '#assistant-effort'].every((id) => {
        const box = document.querySelector(id).getBoundingClientRect();
        return box.width > 10 && box.right <= send.left + 1;
      });
    });
    assert(compact, 'model and effort chips must shrink and remain clear of Send at 300 px');
    // Repeated edits of one file are one step; prose formatting never becomes markup.
    await page.locator('#assistant-tab-claude').click();
    await page.locator('#assistant-input').fill('hello');
    await page.locator('#assistant-input').press('Enter');
    await page.waitForFunction(() => document.querySelector('.assistant-status').textContent === 'Tur tamamlandı');
    assert.equal(await page.locator('#assistant-transcript .assistant-user').count(), 1);
    assert(await page.locator('#assistant-transcript .assistant-step').count() >= 1);
    assert.equal(await page.locator('#assistant-transcript img, #assistant-transcript script').count(), 0);
    assert.equal(await page.locator('.assistant-title').innerText(), 'hello');
    await page.locator('#toggle-assistant').click();
    const closed = await widths();
    assert(Math.abs(closed.left - before.left) <= 1, 'side panel returns to its width when the assistant closes');
    console.log('UI ASSISTANT LAYOUT PASS: no overflow at 1440 px, side panels yield, divider arithmetic, provider tabs, composer footer, grouped steps.');
  } finally {
    await browser.close();
    await server.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
