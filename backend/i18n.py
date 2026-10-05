"""Process-wide server language. English source text is the translation key.

Messages are rendered when produced, including stored job errors and warnings.
The language lock is never held while calling the kernel or writing to stdin.
"""
import threading
from backend.locales.tr import TRANSLATIONS

_lock = threading.RLock()
_language = 'en'


def set_language(value):
    """Set en/tr strictly; reject other values without changing the language."""
    if not isinstance(value, str) or value not in ('en', 'tr'):
        raise ValueError('Invalid language; expected en or tr.')
    global _language
    with _lock:
        _language = value
    return value


def get_language():
    with _lock:
        return _language


def tr(source, **params):
    with _lock:
        template = TRANSLATIONS.get(source, source) if _language == 'tr' else source
    return template.format(**params)
