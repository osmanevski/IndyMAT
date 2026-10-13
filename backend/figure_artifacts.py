"""Read figure artifacts without loading over-budget JSON into memory."""
from contextlib import ExitStack
import errno
import os
from pathlib import Path
import stat
from backend.i18n import tr

JSON_BYTES = 8 * 1024 * 1024


class FigureJSONTooLarge(ValueError):
    def __init__(self, actual):
        super().__init__('Figure JSON exceeds the byte limit.')
        self.reason_args = {'actual': actual, 'limit': JSON_BYTES}


def read_figure_artifact(jobs, job, filename):
    # The route validates the job/filename enums before reaching this reader.
    root = Path(jobs).resolve()
    # Keep the requested job anchor lexical: resolving it would allow one job
    # directory to alias another through a symlink.
    job_root = root / job
    artifact = (job_root / filename).resolve()
    if not artifact.is_relative_to(root) or not artifact.is_relative_to(job_root):
        raise PermissionError(tr('Invalid path.'))
    # Resolve permits existing same-job links, but is only a name check. Pin
    # every resolved directory without following links so a replacement after
    # that check cannot redirect the opened object into a different job.
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    with ExitStack() as opened:
        def open_fd(path, flags, parent=None):
            try:
                fd = os.open(path, flags, dir_fd=parent)
            except OSError as exc:
                if exc.errno in (errno.ELOOP, errno.ENOTDIR):
                    raise PermissionError(tr('Invalid path.')) from exc
                raise
            opened.callback(os.close, fd)
            return fd

        parent = open_fd(root.anchor, directory_flags)
        for component in root.parts[1:]:
            parent = open_fd(component, directory_flags, parent)
        parent = open_fd(job, directory_flags, parent)
        components = artifact.relative_to(job_root).parts
        for component in components[:-1]:
            parent = open_fd(component, directory_flags, parent)
        if not components:
            raise PermissionError(tr('Invalid path.'))
        fd = open_fd(components[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, parent)
        # Check the descriptor, never stat/reopen its mutable pathname. Only
        # regular artifacts are readable (a swapped FIFO must not block HTTP).
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise PermissionError(tr('Invalid path.'))
        is_json = filename.endswith('.json')
        if is_json and info.st_size > JSON_BYTES:
            raise FigureJSONTooLarge(info.st_size)
        with os.fdopen(os.dup(fd), 'rb') as stream:
            data = stream.read(JSON_BYTES + 1) if is_json else stream.read()
        if is_json and len(data) > JSON_BYTES:
            raise FigureJSONTooLarge(len(data))
        return data
