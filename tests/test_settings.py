import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class SettingsIntegrationTests(unittest.TestCase):
 def test_feature_is_local_and_uses_one_versioned_key(self):
  settings=(ROOT/'frontend/settings.js').read_text()
  shortcut=(ROOT/'frontend/shortcut_registry.js').read_text()
  utility=(ROOT/'frontend/shortcut_registry_utils.cjs').read_text()
  index=(ROOT/'static/index.html').read_text()
  self.assertIn('mf-settings-v1',utility)
  self.assertNotIn('/api/',settings)
  self.assertNotIn('/api/',shortcut)
  self.assertIn('id="settings"',index)
 def test_draft_keys_are_not_part_of_settings_migration_writes(self):
  settings=(ROOT/'frontend/settings.js').read_text()
  utility=(ROOT/'frontend/shortcut_registry_utils.cjs').read_text()
  self.assertNotIn('removeItem(',settings)
  self.assertNotIn('removeItem(',utility)
  self.assertNotIn('mf-drafts',settings)
  self.assertNotIn('mf-drafts',utility)
 def test_compatibility_mirrors_precede_queued_commit(self):
  settings=(ROOT/'frontend/settings.js').read_text()
  utility=(ROOT/'frontend/shortcut_registry_utils.cjs').read_text()
  save=settings.split('function saveSettings() {',1)[1].split('function getSetting(',1)[0]
  self.assertLess(save.index('syncPreferenceMirrors('),save.index('writeQueue ='))
  self.assertIn('mf-settings-v1 is authoritative',utility)
  self.assertIn('["theme", "mf-theme"]',utility)
  self.assertIn('["editorFontSize", "mf-editor-font-size"]',utility)



def setUpModule():
 from backend.i18n import set_language
 set_language('tr')
 __import__('os').environ['INDYMAT_LANGUAGE']='tr'  # Octave helpers started directly by a test read this

if __name__ == '__main__':
 unittest.main(verbosity=2)
