#!/usr/bin/env bash
set -euo pipefail

# The orchestrator's job is to run this oracle; the Codex sandbox may not start Octave.
if [[ $# -ne 2 ]]; then
  echo "usage: octave-parse-oracle.sh <corpus_dir> <out.tsv>" >&2
  exit 2
fi
corpus=$(cd "$1" && pwd)
out=$2
command -v octave-cli >/dev/null || { echo "octave-cli not found" >&2; exit 127; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
find "$corpus" -type f -name '*.m' -print0 | sort -z > "$tmp/files0"
python3 - "$tmp/files0" "$tmp/list.txt" <<'PY'
import sys
raw = open(sys.argv[1], 'rb').read().split(b'\0')
with open(sys.argv[2], 'w', encoding='utf-8') as f:
    for item in raw:
        if item:
            f.write(item.decode('utf-8', 'surrogateescape').replace('\\', '\\\\').replace("'", "''") + '\n')
PY
cat > "$tmp/driver.m" <<'M'
fid = fopen(getenv('INDY_CORPUS_FILE_LIST'), 'r');
out = fopen(getenv('INDY_CORPUS_OUT'), 'w');
while true
  path = fgetl(fid);
  if !ischar(path), break; end
  try
    __parse_file__(path);
    status = 'ok'; message = '';
  catch err
    status = 'fail'; message = err.message;
  end
  message = strrep(strrep(strrep(message, sprintf('\t'), ' '), sprintf('\n'), ' '), sprintf('\r'), ' ');
  fprintf(out, '%s\t%s\t%s\n', path, status, message);
end
fclose(fid); fclose(out);
M
INDY_CORPUS_FILE_LIST="$tmp/list.txt" INDY_CORPUS_OUT="$out" octave-cli --no-gui --norc --quiet "$tmp/driver.m"
