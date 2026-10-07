"""Read figure artifacts without loading over-budget JSON into memory."""
from pathlib import Path
from backend.i18n import tr

JSON_BYTES = 8 * 1024 * 1024


class FigureJSONTooLarge(ValueError):
    def __init__(self, actual):
        super().__init__('Figure JSON exceeds the byte limit.')
        self.reason_args = {'actual': actual, 'limit': JSON_BYTES}


def read_figure_artifact(jobs, job, filename):
    # The route validates the job/filename enums before reaching this reader.
    root = Path(jobs).resolve()
    artifact = (root / job / filename).resolve()
    if not artifact.is_relative_to(root):
        raise PermissionError(tr('Invalid path.'))
    if not filename.endswith('.json'):
        return artifact.read_bytes()
    size = artifact.stat().st_size
    if size > JSON_BYTES:
        raise FigureJSONTooLarge(size)
    with artifact.open('rb') as stream:
        # Also bound the read if the producer grew the file after stat.
        data = stream.read(JSON_BYTES + 1)
    if len(data) > JSON_BYTES:
        raise FigureJSONTooLarge(len(data))
    return data
