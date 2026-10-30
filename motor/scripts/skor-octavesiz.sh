#!/bin/sh
# Scoreboard with GNU Octave blocked by the macOS sandbox: proves the engine
# never runs Octave behind the scenes. Usage: motor/scripts/skor-octavesiz.sh [config]
# Exits non-zero if the block does not hold or the scoreboard fails.
set -eu
cd "$(dirname "$0")/../.."
config="${1:-tam}"
real=$(python3 -c "import os,shutil;p=shutil.which('octave-cli');print(os.path.realpath(p) if p else '')")
profile=$(mktemp -t octave-yasak)
{
  echo '(version 1)'
  echo '(allow default)'
  echo '(deny process-exec* (regex #"octave"))'
  if [ -n "$real" ]; then
    root=$(dirname "$(dirname "$real")")
    echo "(deny process-exec* (subpath \"$root\"))"
    echo "(deny file-read-data (subpath \"$root\"))"
  fi
} > "$profile"
if [ -n "$real" ] && sandbox-exec -f "$profile" "$real" --version >/dev/null 2>&1; then
  echo "octave block does not hold: refusing to report a score" >&2
  exit 2
fi
env -i HOME="$HOME" PATH=/usr/bin:/bin sandbox-exec -f "$profile" /usr/bin/python3 motor/scripts/skor.py "$config"
