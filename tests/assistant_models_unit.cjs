const assert = require('node:assert/strict');
const models = require('../frontend/assistant_models_utils.cjs');
const settings = require('../frontend/shortcut_registry_utils.cjs');
const choices = { claude: { model: 'sonnet', effort: 'high' }, codex: { model: 'gpt-6-astra', effort: 'max' }, agy: { model: 'gemini-3.8-flash-high', effort: '' } };
const stored = settings.sanitizeSettings({ version: 1, assistant: { models: choices } });
assert.deepEqual(stored.assistant.models, choices);
const base = settings.cloneDefaults();
const patch = settings.settingsPatch(base, stored);
assert.deepEqual(settings.sanitizeSettings(settings.applyPatch(base, patch)).assistant.models, choices);
for (const value of ['bad id', '--flag', 'x\n', 'a'.repeat(129), 1, null, {}]) {
  assert.deepEqual(models.modelSettings({ claude: { model: value, effort: '' } }), {});
  assert.deepEqual(models.modelSettings({ codex: { model: '', effort: value } }), {});
}
assert.deepEqual(models.modelSettings({ agy: { model: 'listed-model', effort: 'high' } }), { agy: { model: 'listed-model', effort: '' } });
const catalog = { models: [{ id: '', efforts: ['low'] }, { id: 'listed-model', efforts: ['high'] }] };
assert.deepEqual(models.modelChoice(catalog, 'old-model', 'ultra'), { model: '', effort: '' });
assert.deepEqual(models.modelChoice(catalog, 'listed-model', 'high'), { model: 'listed-model', effort: 'high' });
console.log('ASSISTANT MODELS UNIT PASS: provider choices persist through settings patches; malformed and unavailable values fall back.');
