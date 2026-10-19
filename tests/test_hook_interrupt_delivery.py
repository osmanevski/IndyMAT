"""A SIGINT that arrives while the trusted hook attests reaches the reader.

The engine runs like the Kernel's (octave-cli, raw pty input, the real
__mf_debug_prompt__ hook). A breakpoint elsewhere keeps Octave's evaluator in
debug mode, where an interrupt raised inside a statement is caught by that
statement and execution continues with the next one. A native input reader
waits in a function and the hook writes a reader record on every tick.

The signal is placed inside the attestation without timing: the trace path is
replaced by a FIFO whose pipe is already full, so the hook's record write
blocks while the hook holds the trace open. The test sends SIGINT only after
the engine is seen holding the FIFO (libproc), then drains the pipe.
"""
import ctypes
import ctypes.util
import os
import select
import signal
import stat
import subprocess
import tempfile
import termios
import time
import tty
import unittest
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_proc = ctypes.CDLL(ctypes.util.find_library('proc'), use_errno=True)


class _FdInfo(ctypes.Structure):
    _fields_ = [('fd', ctypes.c_int32), ('type', ctypes.c_uint32)]


def _holds_fifo(pid, path):
    """True when the process has a FIFO open at path (vnode path info)."""
    size = _proc.proc_pidinfo(pid, 1, 0, None, 0)  # PROC_PIDLISTFDS
    if size <= 0:
        return False
    items = (_FdInfo*(size//ctypes.sizeof(_FdInfo)))()
    count = _proc.proc_pidinfo(pid, 1, 0, items, size)//ctypes.sizeof(_FdInfo)
    for item in items[:count]:
        if item.type != 1:  # PROX_FDTYPE_VNODE
            continue
        info = ctypes.create_string_buffer(1200)  # vnode_fdinfowithpath
        if _proc.proc_pidfdinfo(pid, item.fd, 2, info, 1200) != 1200:
            continue
        mode = int.from_bytes(info.raw[28:30], 'little')
        name = info.raw[176:].split(b'\0', 1)[0].decode(errors='replace')
        if stat.S_ISFIFO(mode) and os.path.realpath(name) == os.path.realpath(path):
            return True
    return False


class HookInterruptDelivery(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=ROOT, prefix='hook-delivery-')
        self.base = Path(self.temp.name)
        self.trace = self.base/'debug-prompt'
        self.ticket = uuid.uuid4().hex
        (self.base/'delivery_reader.m').write_text(
            "function delivery_reader()\n"
            "  before_reader = 1;\n"
            "  reply = builtin('input', 'DELIVERY_PROMPT:', 's');\n"
            "  printf('AFTER_INPUT[%d]\\n', exist('reply', 'var'));\n"
            "end\n")
        (self.base/'delivery_other.m').write_text("function delivery_other()\n  other_value = 2;\nend\n")
        for name in ('delivery_reader.m', 'delivery_other.m'):
            os.utime(self.base/name, (1_600_000_000, 1_600_000_000))
        master, slave = os.openpty()
        tty.setraw(slave, termios.TCSANOW)
        env = os.environ.copy()
        env.update(INDYMAT_LANGUAGE='en', INPUTRC=str(ROOT/'octave'/'readline.inputrc'))
        self.proc = subprocess.Popen(
            ['octave-cli', '--no-gui', '--quiet', '--no-init-file', '--no-site-file', '--no-history',
             '--line-editing', '--interactive'],
            stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, cwd=self.base, env=env,
            start_new_session=True, bufsize=0)
        os.close(slave)
        self.stdin = os.fdopen(master, 'wb', buffering=0)
        self.output = bytearray()
        quoted = lambda value: "'"+str(value).replace("'", "''")+"'"
        self.send("more off; page_screen_output(false); crash_dumps_octave_core(false); "
                  "warning('off','Octave:shadowed-function'); PS1(''); PS2(''); "
                  f"addpath({quoted(ROOT/'octave')}); addpath({quoted(self.base)}); "
                  f"(@__mf_debug_prompt__)({quoted(self.trace)}); "
                  f"setenv('INDYMAT_DEBUG_TICKET',{quoted(self.ticket)}); disp('HARNESS_READY')")
        self.read_until('HARNESS_READY')

    def tearDown(self):
        try:
            os.killpg(self.proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        self.proc.wait()
        self.stdin.close()
        self.proc.stdout.close()
        self.temp.cleanup()

    def send(self, line):
        self.stdin.write((line+'\n').encode())

    def read_until(self, needle, seconds=15):
        end = time.monotonic()+seconds
        while needle.encode() not in self.output and time.monotonic() < end:
            ready, _, _ = select.select([self.proc.stdout], [], [], .05)
            if ready:
                data = os.read(self.proc.stdout.fileno(), 65536)
                if not data:
                    break
                self.output += data
        return needle.encode() in self.output

    def reader_records(self):
        prefix = f'Q {self.ticket} 0 '
        return [r for r in self.trace.read_text().splitlines()
                if r.startswith(prefix) and r[len(prefix):].isdigit() and int(r[len(prefix):]) > 0]

    def interrupt_during_attestation(self):
        end = time.monotonic()+15
        while len(self.reader_records()) < 2:
            self.assertLess(time.monotonic(), end, self.output)
            time.sleep(.01)
        fifo = self.base/'attestation-fifo'
        os.mkfifo(fifo)
        drain = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
        fill = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
        try:
            for chunk in (4096, 1):
                while True:
                    try:
                        os.write(fill, b'.'*chunk)
                    except BlockingIOError:
                        break
            os.replace(fifo, self.trace)
            end = time.monotonic()+15
            while not _holds_fifo(self.proc.pid, self.trace):
                self.assertLess(time.monotonic(), end, 'hook never reached the attestation write')
                time.sleep(.005)
            # The record write cannot complete: the hook holds the trace open.
            os.killpg(self.proc.pid, signal.SIGINT)
            drained = bytearray()
            end = time.monotonic()+15
            while _holds_fifo(self.proc.pid, self.trace):
                self.assertLess(time.monotonic(), end, 'attestation did not finish')
                try:
                    drained += os.read(drain, 65536)
                except BlockingIOError:
                    time.sleep(.005)
            restore = self.base/'restored-trace'
            restore.write_text('')
            os.replace(restore, self.trace)
            try:
                drained += os.read(drain, 65536)
            except BlockingIOError:
                pass
            # Exactly one whole record reached the channel: nothing partial.
            self.assertEqual(bytes(drained).lstrip(b'.'), f'Q {self.ticket} 0 1\n'.encode())
        finally:
            os.close(fill)
            os.close(drain)

    def assert_reader_aborted(self):
        delivered = self.read_until('AFTER_INPUT[', seconds=5)
        if not delivered:
            # Evidence for the failure: the reader survived and takes a reply.
            self.send('reader survived the signal')
            self.read_until('AFTER_INPUT[', seconds=5)
        print('HOOK_DELIVERY', self.id(), bytes(self.output[-300:]), flush=True)
        self.assertTrue(delivered, 'SIGINT inside the trusted hook did not reach the input reader')
        self.assertIn(b'AFTER_INPUT[0]', self.output)
        self.assert_channel_closed()

    def assert_channel_closed(self):
        # No attestation descriptor or descriptor note outlives the interrupt.
        self.send("printf('CHANNEL[%d %d]\\n', numel(fopen('all')), numel(getenv('INDYMAT_DEBUG_FID')))")
        self.assertTrue(self.read_until('CHANNEL['), self.output)
        self.assertIn(b'CHANNEL[0 0]', self.output)

    def test_signal_inside_attestation_reaches_reader_in_debug_mode(self):
        self.send("dbstop delivery_other 2; disp('BREAKPOINT_SET')")
        self.assertTrue(self.read_until('BREAKPOINT_SET'))
        self.send('delivery_reader();')
        self.assertTrue(self.read_until('DELIVERY_PROMPT:'))
        self.interrupt_during_attestation()
        self.assert_reader_aborted()

    def test_signal_inside_attestation_reaches_reader_without_breakpoints(self):
        self.send('delivery_reader();')
        self.assertTrue(self.read_until('DELIVERY_PROMPT:'))
        self.interrupt_during_attestation()
        # Outside debug mode the aborted reader also aborts its caller. A
        # surviving reader would take this line as its reply instead.
        self.send("printf('ROOT_AFTER[%d]\\n', exist('before_reader','var'))")
        self.assertTrue(self.read_until('ROOT_AFTER['), self.output)
        self.assertNotIn(b'AFTER_INPUT[', self.output)
        self.assertIn(b'ROOT_AFTER[0]', self.output)
        self.assert_channel_closed()


if __name__ == '__main__':
    unittest.main()
