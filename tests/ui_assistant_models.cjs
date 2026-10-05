const assert = require('node:assert/strict');
const { chromium } = require('playwright');
const { assistantServer } = require('./assistant_harness.cjs');
(async () => {
  const server = await assistantServer();
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({ locale: 'tr-TR', viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    let release;
    const held = new Promise((resolve) => { release = resolve; });
    await page.route(/\/api\/assistant\/models\?provider=claude$/, async (route) => {
      await held;
      await route.continue();
    });
    await page.goto(server.launch.url, { waitUntil: 'domcontentloaded' });
    await page.locator('.cm-line').first().waitFor();
    await page.locator('#toggle-assistant').click();
    assert.equal(await page.locator('#assistant-model').getAttribute('aria-busy'), 'true');
    assert.equal(await page.locator('#assistant-model').isDisabled(), true);
    assert.equal(await page.locator('#assistant-model option').first().textContent(), 'Model: varsayılan');
    release();
    await page.waitForFunction(() => document.querySelector('#assistant-model').getAttribute('aria-busy') === 'false');
    await page.locator('#assistant-model').selectOption('sonnet');
    await page.locator('#assistant-effort').selectOption('high');
    assert.equal(await page.locator('#assistant-effort option:checked').textContent(), 'Yüksek');
    await page.waitForFunction(() => JSON.parse(localStorage.getItem('mf-settings-v1')).assistant.models.claude.model === 'sonnet');
    // Reload creates a new page of conversations while retaining the provider's new-conversation preference.
    await page.evaluate(() => {
      const settings = JSON.parse(localStorage.getItem('mf-settings-v1'));
      settings.assistant.width = 300;
      localStorage.setItem('mf-settings-v1', JSON.stringify(settings));
    });
    await page.reload();
    await page.waitForFunction(() => document.querySelector('#assistant-model')?.value === 'sonnet');
    assert.equal(await page.locator('#assistant-effort').inputValue(), 'high');
    const noOverlap = async () => {
      const boxes = await page.evaluate(() => {
        const panel = document.querySelector('#assistant-panel').getBoundingClientRect();
        const buttons = ['#assistant-send', '#assistant-stop'].map((id) => document.querySelector(id)).filter((button) => !button.hidden).map((button) => button.getBoundingClientRect());
        const selects = [...document.querySelectorAll('.assistant-selectors select')].filter((select) => !select.hidden && select.id !== 'assistant-provider').map((select) => select.getBoundingClientRect());
        return { width: panel.width, okay: selects.every((box) => box.width > 10 && box.left >= panel.left && box.right <= panel.right && buttons.every((button) => box.right <= button.left + 1)) };
      });
      assert(Math.abs(boxes.width - 300) <= 1, JSON.stringify(boxes));
      assert(boxes.okay, 'footer choices must remain visible and clear of send/stop at 300 px');
    };
    await noOverlap();
    const send = async (prompt) => {
      await page.locator('#assistant-input').fill(prompt);
      await page.locator('#assistant-input').press('Enter');
      await page.waitForFunction(() => document.querySelector('#assistant-model').disabled && document.querySelector('#assistant-provider').disabled);
      await page.waitForFunction(() => document.querySelector('.assistant-status').textContent === 'Tur tamamlandı');
    };
    await send('config');
    assert.equal(await page.locator('#assistant-model').isDisabled(), true);
    assert.equal(await page.locator('#assistant-effort').isDisabled(), true);
    assert.equal(await page.locator('.assistant-model-suffix').textContent(), ' · Sonnet');
    await noOverlap();
    await page.getByRole('button', { name: 'Yeni görüşme', exact: true }).click();
    await page.locator('#assistant-tab-codex').click();
    await page.waitForFunction(() => [...document.querySelector('#assistant-model').options].some((option) => option.value === 'gpt-6-astra'));
    await page.locator('#assistant-model').selectOption('gpt-6-astra');
    await page.locator('#assistant-effort').selectOption('high');
    await send('config');
    assert.equal(await page.locator('#assistant-model').isDisabled(), true);
    assert.equal(await page.locator('#assistant-effort').isDisabled(), false);
    await page.locator('#assistant-effort').selectOption('low');
    await send('config');
    assert.equal(await page.locator('.assistant-model-suffix').textContent(), ' · GPT-6 Astra');
    await noOverlap();
    await page.getByRole('button', { name: 'Yeni görüşme', exact: true }).click();
    await page.locator('#assistant-tab-agy').click();
    await page.waitForFunction(() => [...document.querySelector('#assistant-model').options].some((option) => option.value === 'gemini-3.8-flash-high'));
    assert.equal(await page.locator('#assistant-effort').isHidden(), true);
    await page.locator('#assistant-model').selectOption('gemini-3.8-flash-high');
    await send('config');
    assert.equal(await page.locator('#assistant-model').isDisabled(), true);
    await noOverlap();
    await page.getByRole('button', { name: 'Yeni görüşme', exact: true }).click();
    await page.locator('#assistant-tab-claude').click();
    await page.locator('#assistant-input').fill('wait');
    await page.locator('#assistant-input').press('Enter');
    await page.waitForFunction(() => !document.querySelector('#assistant-stop').hidden && !document.querySelector('#assistant-stop').disabled);
    await noOverlap();
    await page.locator('#assistant-stop').click();
    await page.waitForFunction(() => document.querySelector('.assistant-status').textContent === 'Durduruldu');
    // A failed discovery remains usable with Default and a plain-text explanatory title.
    const fallback = await context.newPage();
    await fallback.route(/\/api\/assistant\/models\?provider=claude$/, (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ models: [{ id: '', label: 'Default', efforts: [] }], efforts_separate: true, note: 'Fixture discovery unavailable' }) }));
    await fallback.goto(server.launch.url);
    await fallback.locator('#assistant-model').waitFor({ state: 'attached' });
    await fallback.waitForFunction(() => document.querySelector('#assistant-model').getAttribute('aria-busy') === 'false');
    assert.equal(await fallback.locator('#assistant-model option').count(), 1);
    assert.equal(await fallback.locator('#assistant-model').getAttribute('title'), 'Fixture discovery unavailable');
    console.log('UI ASSISTANT MODELS PASS: Turkish loading, choices, reload persistence, fixed models, per-turn Codex effort, title labels, 300 px footer and discovery fallback.');
  } finally {
    await browser.close();
    await server.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
