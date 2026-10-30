"""Shared by the Rust-engine test files: find or build the engine, skip the file when neither is possible.

The default GNU Octave path never needs this: a machine without a Rust toolchain still passes the checks.
"""
import shutil
import subprocess
import unittest
from pathlib import Path

from backend.kernel_rust import kernel_executable

ROOT = Path(__file__).resolve().parents[1]


def require_rust_kernel():
    try:
        return kernel_executable(ROOT)
    except RuntimeError:
        pass
    if not shutil.which('cargo'):
        raise unittest.SkipTest('The Rust engine is not built and cargo is not installed.')
    subprocess.run(['cargo', 'build', '--offline', '-p', 'indy-kernel'], cwd=ROOT / 'motor', check=True)
    return kernel_executable(ROOT)
